//! Compose's `DateRangeInputTest`, at density 1 in a 400 by 800 window, on the `String` fields
//! until `text-field-state` (D1562).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_material3::calendar_model::CalendarModel;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{FocusRequester, Locale, SemanticsProperties};

fn day_in_utc_millis(year: i32, month: i32, day: i32) -> i64 {
    CalendarModel::new(Locale::new("en-US")).get_month(year, month).start_utc_time_millis
        + i64::from(day - 1) * 86_400_000
}

thread_local! {
    static STATE: Cell<Option<DateRangePickerState>> = const { Cell::new(None) };
}

fn state() -> DateRangePickerState {
    STATE.with(Cell::get).unwrap()
}

fn range_picker(make: fn() -> DateRangePickerState) -> ComposeTestRule {
    compose_test(move || {
        MaterialTheme!({
            let state = make();
            STATE.with(|cell| cell.set(Some(state)));
            DateRangePicker!(state);
        });
    })
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

fn enter(ui: &mut ComposeTestRule, label: &str, digits: &str) {
    ui.on_node_with_text(label).perform_click().perform_text_input(digits);
    ui.wait_for_idle();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangeInputTest.kt, dateRangeInput
#[test]
fn typed_dates_are_the_range_and_leave_the_headlines_placeholders() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2019, 1, 1)),
            initial_display_mode = DisplayMode::Input
        )
    });
    ui.wait_for_idle();
    ui.on_all_nodes_unmerged(has_text("Start date")).assert_count_equals(2);
    ui.on_all_nodes_unmerged(has_text("End date")).assert_count_equals(2);

    enter(&mut ui, "Start date", "01272019");
    enter(&mut ui, "End date", "05102020");
    assert_eq!(state().selected_start_date_millis(), Some(day_in_utc_millis(2019, 1, 27)));
    assert_eq!(state().selected_end_date_millis(), Some(day_in_utc_millis(2020, 5, 10)));

    ui.on_all_nodes_unmerged(has_text("Start date")).assert_count_equals(1);
    ui.on_all_nodes_unmerged(has_text("End date")).assert_count_equals(1);
    ui.on_node_with_text_unmerged("Jan 27, 2019").assert_exists();
    ui.on_node_with_text_unmerged("May 10, 2020").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangeInputTest.kt, dateRangeInputWithInitialDates
#[test]
fn the_initial_dates_are_in_the_fields_and_the_headline() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(day_in_utc_millis(2010, 5, 11)),
            initial_selected_end_date_millis = Some(day_in_utc_millis(2020, 10, 20)),
            initial_display_mode = DisplayMode::Input
        )
    });
    ui.on_node_with_text("05/11/2010").assert_exists();
    ui.on_node_with_text("10/20/2020").assert_exists();
    ui.on_node_with_text_unmerged("May 11, 2010").assert_exists();
    ui.on_node_with_text_unmerged("Oct 20, 2020").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangeInputTest.kt, dateRangeInput_initialFocusOnInputField
#[test]
fn only_the_start_field_takes_the_focus_after_a_while() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(day_in_utc_millis(2010, 5, 11)),
            initial_selected_end_date_millis = Some(day_in_utc_millis(2020, 10, 20)),
            initial_display_mode = DisplayMode::Input
        )
    });
    ui.advance_time_by(Duration::from_millis(1000));
    ui.wait_for_idle();
    ui.on_node_with_text("05/11/2010").assert_is_focused();
    ui.on_node_with_text("10/20/2020").assert_is_not_focused();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangeInputTest.kt, dateRangeInput_noInitialFocusOnInputField
#[test]
fn no_requester_leaves_both_fields_unfocused() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            DateRangePicker!(
                remember_date_range_picker_state!(
                    initial_selected_start_date_millis = Some(day_in_utc_millis(2010, 5, 11)),
                    initial_selected_end_date_millis = Some(day_in_utc_millis(2020, 10, 20)),
                    initial_display_mode = DisplayMode::Input
                ),
                focus_requester = None::<FocusRequester>
            );
        });
    });
    ui.advance_time_by(Duration::from_millis(1000));
    ui.wait_for_idle();
    ui.on_node_with_text("05/11/2010").assert_is_not_focused();
    ui.on_node_with_text("10/20/2020").assert_is_not_focused();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangeInputTest.kt, dateRangeInputWithInitialDate_alternateLocale
#[test]
fn the_fields_go_by_the_states_locale_and_the_headline_by_the_default_one() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            let state = remember(|| {
                DateRangePickerState::new(
                    Locale::new("he"),
                    Some(day_in_utc_millis(2010, 5, 11)),
                    Some(day_in_utc_millis(2020, 5, 20)),
                    None,
                    DatePickerDefaults::YEAR_RANGE,
                    DisplayMode::Input,
                    DatePickerDefaults::all_dates(),
                )
            });
            DateRangePicker!(state);
        });
    });
    ui.on_node_with_text("11.05.2010").assert_exists();
    ui.on_node_with_text("20.05.2020").assert_exists();
    ui.on_node_with_text_unmerged("May 11, 2010").assert_exists();
    ui.on_node_with_text_unmerged("May 20, 2020").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangeInputTest.kt, inputDateNotAllowed
#[test]
fn dates_that_are_not_allowed_are_errors_and_select_nothing() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_display_mode = DisplayMode::Input,
            selectable_dates = remember(|| Rc::new(NoDates) as Rc<dyn SelectableDates>)
        )
    });
    enter(&mut ui, "Start date", "01272019");
    enter(&mut ui, "End date", "05102020");
    assert_eq!(state().selected_start_date_millis(), None);
    assert_eq!(state().selected_end_date_millis(), None);
    ui.on_node_with_text("01/27/2019").assert(has_error("Date not allowed: Jan 27, 2019"));
    ui.on_node_with_text("05/10/2020").assert(has_error("Date not allowed: May 10, 2020"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangeInputTest.kt, outOfOrderDateRange
#[test]
fn an_end_before_the_start_is_an_error_and_keeps_the_start() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            year_range = 2018..=2023,
            initial_display_mode = DisplayMode::Input
        )
    });
    enter(&mut ui, "Start date", "01272020");
    enter(&mut ui, "End date", "05102019");
    assert_eq!(state().selected_start_date_millis(), Some(day_in_utc_millis(2020, 1, 27)));
    assert_eq!(state().selected_end_date_millis(), None);
    ui.on_node_with_text_unmerged("05/10/2019").assert(has_error("Invalid date range input"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangeInputTest.kt, switchToDateRangePicker
#[test]
fn the_toggle_goes_to_the_calendar() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(initial_display_mode = DisplayMode::Input)
    });
    ui.wait_for_idle();
    ui.on_node_with_content_description("Switch to calendar input mode").perform_click();
    ui.wait_for_idle();
    ui.on_node_with_text_unmerged("Start date").assert_is_displayed();
    ui.on_node_with_text_unmerged("End date").assert_is_displayed();
    // The fields' labels are gone: all that's left is the headline's, which is cleared.
    ui.on_node_with_text("Start date").assert_does_not_exist();
    ui.on_node_with_text("End date").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangeInputTest.kt, defaultSemantics
#[test]
fn the_headline_reads_out_both_dates_in_full() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(day_in_utc_millis(2010, 5, 11)),
            initial_selected_end_date_millis = Some(day_in_utc_millis(2010, 6, 12)),
            initial_display_mode = DisplayMode::Input
        )
    });
    ui.on_node_with_content_description(
        "Start date: Tuesday, May 11, 2010, End date: Saturday, June 12, 2010",
    )
    .assert_exists();
}
