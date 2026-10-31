//! Compose's `DateInputTest`, at density 1 in a 400 by 800 window.

use std::cell::Cell;
use std::rc::Rc;

use compose_material3::calendar_model::CalendarModel;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{FocusRequester, Key, Locale, Modifier, SemanticsProperties};

fn day_in_utc_millis(year: i32, month: i32, day: i32) -> i64 {
    CalendarModel::new(Locale::new("en-US")).get_month(year, month).start_utc_time_millis
        + i64::from(day - 1) * 86_400_000
}

thread_local! {
    static STATE: Cell<Option<DatePickerState>> = const { Cell::new(None) };
}

fn state() -> DatePickerState {
    STATE.with(Cell::get).unwrap()
}

fn picker(make: fn() -> DatePickerState) -> ComposeTestRule {
    compose_test(move || {
        MaterialTheme!({
            let state = make();
            STATE.with(|cell| cell.set(Some(state)));
            DatePicker!(state);
        });
    })
}

fn with_date(millis: i64) -> Option<i64> {
    Some(millis)
}

fn has_error(description: &str) -> Matcher {
    let description = description.to_string();
    Matcher::new(format!("with the error {description:?}"), move |config| {
        config
            .get(&SemanticsProperties::ERROR)
            .is_some_and(|error| *error.to_string() == *description)
    })
}

struct NoDates;
impl SelectableDates for NoDates {
    fn is_selectable_date(&self, _utc_time_millis: i64) -> bool {
        false
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, dateInput
#[test]
fn a_typed_date_is_selected() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(Some(day_in_utc_millis(2019, 1, 1))),
            initial_display_mode = DisplayMode::Input
        )
    });
    ui.on_node_with_text("Entered date").assert_exists();
    ui.on_node_with_text("Date").perform_click().perform_text_input("01272019");
    ui.wait_for_idle();
    assert_eq!(state().selected_date_millis(), Some(day_in_utc_millis(2019, 1, 27)));
    ui.on_node_with_text("Entered date").assert_does_not_exist();
    ui.on_node_with_text("Jan 27, 2019").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, dateInputWithInitialDate
#[test]
fn the_initial_date_is_in_the_field_and_the_headline() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_selected_date_millis = with_date(day_in_utc_millis(2010, 5, 11)),
            initial_display_mode = DisplayMode::Input
        )
    });
    ui.on_node_with_text("05/11/2010").assert_exists();
    ui.on_node_with_text("May 11, 2010").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, dateInput_initialFocusOnInputField
#[test]
fn the_field_takes_the_focus_after_a_while() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_selected_date_millis = with_date(day_in_utc_millis(2010, 5, 11)),
            initial_display_mode = DisplayMode::Input
        )
    });
    ui.advance_time_by(std::time::Duration::from_millis(1000));
    ui.wait_for_idle();
    ui.on_node_with_text("05/11/2010").assert_is_focused();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, dateInput_noInitialFocusOnInputField
#[test]
fn no_requester_leaves_the_focus_alone() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            DatePicker!(
                remember_date_picker_state!(
                    initial_selected_date_millis = with_date(day_in_utc_millis(2010, 5, 11)),
                    initial_display_mode = DisplayMode::Input
                ),
                focus_requester = None::<FocusRequester>
            );
        });
    });
    ui.advance_time_by(std::time::Duration::from_millis(1000));
    ui.wait_for_idle();
    ui.on_node_with_text("05/11/2010").assert_is_not_focused();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, dateInputWithInitialDate_hebrewLocale
#[test]
fn a_locales_format_goes_by_the_states_locale() {
    let state = DatePickerState::new(
        Locale::new("he"),
        Some(day_in_utc_millis(2010, 5, 11)),
        Some(day_in_utc_millis(2010, 5, 11)),
        DatePickerDefaults::YEAR_RANGE,
        DisplayMode::Input,
        DatePickerDefaults::all_dates(),
    );
    let state = Rc::new(state);
    let mut ui = compose_test(move || {
        let state = **state;
        MaterialTheme!({
            DatePicker!(state);
        });
    });
    ui.on_node_with_text("11.05.2010").assert_exists();
    // The headline goes by the default locale.
    ui.on_node_with_text("May 11, 2010").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, dateInputWithInitialDate_arabLocale
#[test]
fn a_locales_digits_are_in_the_field() {
    let state = DatePickerState::new(
        Locale::new("ar-u-nu-arab"),
        Some(day_in_utc_millis(2022, 9, 10)),
        Some(day_in_utc_millis(2022, 9, 10)),
        DatePickerDefaults::YEAR_RANGE,
        DisplayMode::Input,
        DatePickerDefaults::all_dates(),
    );
    let state = Rc::new(state);
    let mut ui = compose_test(move || {
        let state = **state;
        MaterialTheme!({
            DatePicker!(state);
        });
    });
    ui.on_node_with_text("١٠/٠٩/٢٠٢٢").assert_exists();
    ui.on_node_with_text("Sep 10, 2022").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, dateInputWithInitialDate_externalDateChange
#[test]
fn a_date_set_on_the_state_replaces_the_field() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_selected_date_millis = with_date(day_in_utc_millis(2010, 5, 11)),
            initial_display_mode = DisplayMode::Input
        )
    });
    ui.on_node_with_text("May 11, 2010").assert_exists();
    ui.on_node_with_text("05/11/2010").assert_exists();
    let later = day_in_utc_millis(2023, 3, 9);
    state().set_selected_date_millis(Some(later));
    state().set_displayed_month_millis(later);
    ui.wait_for_idle();
    ui.on_node_with_text("May 11, 2010").assert_does_not_exist();
    ui.on_node_with_text("05/11/2010").assert_does_not_exist();
    ui.on_node_with_text("03/09/2023").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, inputDateNotAllowed
#[test]
fn a_date_not_allowed_is_an_error() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_display_mode = DisplayMode::Input,
            selectable_dates = remember(|| Rc::new(NoDates) as Rc<dyn SelectableDates>)
        )
    });
    ui.on_node_with_text("Date").perform_click().perform_text_input("02272020");
    ui.wait_for_idle();
    assert_eq!(state().selected_date_millis(), None);
    ui.on_node_with_text("02/27/2020").assert(has_error("Date not allowed: Feb 27, 2020"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, inputDateOutOfRange
#[test]
fn a_date_out_of_the_years_is_an_error() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            year_range = 2018..=2023,
            initial_display_mode = DisplayMode::Input
        )
    });
    ui.on_node_with_text("Date").perform_click().perform_text_input("02272030");
    ui.wait_for_idle();
    assert_eq!(state().selected_date_millis(), None);
    ui.on_node_with_text("02/27/2030")
        .assert(has_error("Date out of expected year range 2018 - 2023"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, inputDateOutOfRange_withInitialDate
#[test]
fn an_initial_date_not_allowed_is_an_error_and_stays() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_display_mode = DisplayMode::Input,
            initial_selected_date_millis = with_date(day_in_utc_millis(2030, 2, 27)),
            selectable_dates = remember(|| Rc::new(NoDates) as Rc<dyn SelectableDates>)
        )
    });
    assert_eq!(state().selected_date_millis(), Some(1898380800000));
    ui.on_node_with_text("Date").assert_is_displayed();
    ui.on_node_with_text("02/27/2030").assert(has_error("Date not allowed: Feb 27, 2030"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, inputDateInvalidForPattern
#[test]
fn what_isnt_a_date_is_an_error() {
    let mut ui = picker(|| remember_date_picker_state!(initial_display_mode = DisplayMode::Input));
    ui.on_node_with_text("Date").perform_click().perform_text_input("99272030");
    ui.wait_for_idle();
    assert_eq!(state().selected_date_millis(), None);
    ui.on_node_with_text("99/27/2030")
        .assert(has_error("Date does not match expected pattern: MM/DD/YYYY"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, switchToDatePicker
#[test]
fn the_toggle_goes_to_the_calendar() {
    let mut ui = picker(|| remember_date_picker_state!(initial_display_mode = DisplayMode::Input));
    ui.on_node_with_content_description("Switch to calendar input mode").perform_click();
    ui.wait_for_idle();
    ui.on_node(has_content_description_with("next", true, true)).assert_is_displayed();
    ui.on_node(has_content_description_with("previous", true, true)).assert_is_displayed();
    ui.on_node_with_text("Date").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, heightUnchangedWithError
#[test]
fn the_height_stays_with_an_error() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            DatePicker!(
                remember_date_picker_state!(initial_display_mode = DisplayMode::Input),
                modifier = Modifier.test_tag("DateInput")
            );
        });
    });
    let without = ui.on_node_with_tag("DateInput").fetch_semantics_snapshot().bounds.height();
    ui.on_node_with_text("Date").perform_click().perform_text_input("12123000");
    ui.wait_for_idle();
    let with = ui.on_node_with_tag("DateInput").fetch_semantics_snapshot().bounds.height();
    assert_eq!(without, with);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, defaultSemantics
#[test]
fn the_headline_reads_the_entered_date() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_selected_date_millis = with_date(day_in_utc_millis(2010, 5, 11)),
            initial_display_mode = DisplayMode::Input
        )
    });
    ui.on_node_with_text("May 11, 2010")
        .assert_content_description_equals(&["Entered date: Tuesday, May 11, 2010"]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateInputTest.kt, dateInput_delimiterInsertedImmediately
#[test]
fn a_delimiter_shows_as_soon_as_what_comes_before_it_is_typed() {
    let mut ui = picker(|| remember_date_picker_state!(initial_display_mode = DisplayMode::Input));
    ui.on_node_with_text("Date").perform_click().perform_text_input("01");
    ui.on_node_with_text("01/").assert_exists();
    ui.on_node_with_text("01/").perform_text_input("27");
    ui.on_node_with_text("01/27/").assert_exists();
}

#[test]
fn only_digits_up_to_the_patterns_length_go_in() {
    let mut ui = picker(|| remember_date_picker_state!(initial_display_mode = DisplayMode::Input));
    ui.on_node_with_text("Date").perform_click().perform_text_input("0127");
    ui.on_node_with_text("01/27/").perform_text_input("x");
    ui.on_node_with_text("01/27/").assert_exists();
    ui.on_node_with_text("01/27/").perform_text_input("2019");
    ui.wait_for_idle();
    assert_eq!(state().selected_date_millis(), Some(day_in_utc_millis(2019, 1, 27)));
    ui.on_node_with_text("01/27/2019").perform_text_input("5");
    ui.on_node_with_text("01/27/2019").assert_exists();
}

#[test]
fn a_date_cut_short_is_no_selection() {
    let mut ui = picker(|| remember_date_picker_state!(initial_display_mode = DisplayMode::Input));
    ui.on_node_with_text("Date").perform_click().perform_text_input("01272019");
    ui.wait_for_idle();
    assert_eq!(state().selected_date_millis(), Some(day_in_utc_millis(2019, 1, 27)));
    ui.on_node_with_text("01/27/2019").perform_key_input(|keys| {
        keys.press_key(Key::Backspace);
        keys.press_key(Key::Backspace);
    });
    ui.wait_for_idle();
    assert_eq!(state().selected_date_millis(), None);
    ui.on_node_with_text("01/27/20").assert_exists();
}
