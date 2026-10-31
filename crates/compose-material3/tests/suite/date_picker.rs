//! Compose's `DatePickerTest`, at density 1 in a 400 by 800 window. The `java.time` forms
//! (`getSelectedDate`, `setDisplayedMonth`, `rememberDatePickerState(initialSelectedDate = …)`) have
//! no counterpart: there's no `java.time` here.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_material3::calendar_model::CalendarModel;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, Density, IntSize, Key, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Locale, Modifier,
    SemanticsProperties, TextStyle,
};

fn model() -> CalendarModel {
    CalendarModel::new(Locale::new("en-US"))
}

fn day_in_utc_millis(year: i32, month: i32, day: i32) -> i64 {
    model().get_month(year, month).start_utc_time_millis + i64::from(day - 1) * 86_400_000
}

thread_local! {
    static STATE: Cell<Option<DatePickerState>> = const { Cell::new(None) };
}

fn state() -> DatePickerState {
    STATE.with(Cell::get).unwrap()
}

/// A date picker on the state `make` remembers, in the theme.
fn picker(make: fn() -> DatePickerState) -> ComposeTestRule {
    compose_test(move || {
        MaterialTheme!({
            let state = make();
            STATE.with(|cell| cell.set(Some(state)));
            DatePicker!(state);
        });
    })
}

/// A date picker in a dialog with its buttons, on the 11th of January 2010.
fn picker_in_dialog(direction: LayoutDirection) -> ComposeTestRule {
    thread_local! { static DIRECTION: Cell<LayoutDirection> = const { Cell::new(LayoutDirection::Ltr) }; }
    DIRECTION.with(|cell| cell.set(direction));
    compose_test(|| {
        MaterialTheme!({
            CompositionLocalProvider(
                LOCAL_LAYOUT_DIRECTION.provides(DIRECTION.with(Cell::get)),
                || {
                    let state = remember_date_picker_state!(
                        initial_selected_date_millis = Some(day_in_utc_millis(2010, 1, 11)),
                        initial_displayed_month_millis = Some(day_in_utc_millis(2010, 1, 1))
                    );
                    DatePickerDialog!(
                        || {},
                        confirm_button = {
                            TextButton!(|| {}, {
                                Text!("OK");
                            });
                        },
                        dismiss_button = Some(|| {
                            TextButton!(|| {}, {
                                Text!("Cancel");
                            });
                        }),
                        |_| {
                            DatePicker!(state);
                        }
                    );
                },
            );
        });
    })
}

fn next() -> Matcher {
    has_content_description_with("next", true, true)
}

fn previous() -> Matcher {
    has_content_description_with("previous", true, true)
}

fn day(text: &str) -> Matcher {
    has_text_containing(text).and(has_click_action())
}

struct NoDates;
impl SelectableDates for NoDates {
    fn is_selectable_date(&self, _utc_time_millis: i64) -> bool {
        false
    }
}

struct NoYears;
impl SelectableDates for NoYears {
    fn is_selectable_year(&self, _year: i32) -> bool {
        false
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, dateSelectionWithInitialDate
#[test]
fn the_initial_date_is_selected_and_in_the_headline() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_selected_date_millis = Some(day_in_utc_millis(2010, 5, 11)),
            initial_displayed_month_millis = Some(day_in_utc_millis(2010, 5, 1))
        )
    });
    ui.on_node(day("11")).assert_is_selected();
    ui.on_node_with_text("May 11, 2010").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, dateSelection
#[test]
fn a_click_selects_a_day() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2019, 1, 1))
        )
    });
    ui.on_node_with_text("Selected date").assert_exists();
    ui.on_node(day("27")).assert_is_not_selected();
    ui.on_node(day("27")).perform_click();
    ui.wait_for_idle();
    assert_eq!(state().selected_date_millis(), Some(day_in_utc_millis(2019, 1, 27)));
    ui.on_node_with_text("Selected date").assert_does_not_exist();
    ui.on_node_with_text("Jan 27, 2019").assert_exists();
    ui.on_node(day("27")).assert_is_selected();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, blockedDateSelection
#[test]
fn a_blocked_date_is_not_selected() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2019, 1, 1)),
            selectable_dates = remember(|| Rc::new(NoDates) as Rc<dyn SelectableDates>)
        )
    });
    ui.on_node_with_text("Selected date").assert_exists();
    ui.on_node(day("27")).perform_click();
    ui.wait_for_idle();
    assert_eq!(state().selected_date_millis(), None);
    ui.on_node_with_text("Selected date").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, blockedYearSelection
#[test]
fn a_blocked_year_is_disabled() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2019, 1, 1)),
            selectable_dates = remember(|| Rc::new(NoYears) as Rc<dyn SelectableDates>)
        )
    });
    ui.on_node_with_text("January 2019").perform_click();
    ui.wait_for_idle();
    for year in 2019..=2031 {
        ui.on_node_with_text(&format!("Navigate to year {year}")).assert_is_not_enabled();
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, selectableDates_updatedSelection
#[test]
fn new_selectable_dates_take_effect() {
    struct AllDates;
    impl SelectableDates for AllDates {}
    let mut ui = compose_test(|| {
        MaterialTheme!({
            let selectable = remember(|| mutable_state_of(false));
            let dates: Rc<dyn SelectableDates> = remember_keyed(selectable.get(), || {
                if selectable.get() {
                    Rc::new(NoDates) as Rc<dyn SelectableDates>
                } else {
                    Rc::new(AllDates)
                }
            });
            Column!({
                let state = remember_date_picker_state!(
                    initial_displayed_month_millis = Some(day_in_utc_millis(2019, 1, 1)),
                    selectable_dates = dates.clone()
                );
                DatePicker!(state);
                Button!(
                    move || selectable.set(true),
                    modifier = Modifier.test_tag("disableSelection"),
                    {
                        Text!("Disable selection");
                    }
                );
            });
        });
    });
    ui.on_node(day("27")).assert_is_enabled();
    ui.on_node_with_tag("disableSelection").perform_click();
    ui.wait_for_idle();
    ui.on_node(day("27")).assert_is_not_enabled();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, yearSelection
#[test]
fn a_year_is_picked_in_the_grid() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2019, 1, 1))
        )
    });
    ui.on_node_with_text("January 2019").perform_click();
    ui.on_node_with_text("Navigate to year 2019").assert_is_selected();
    ui.on_node_with_text("Navigate to year 2020").perform_click();
    ui.on_all_nodes(day("15")).on_first().perform_click();
    ui.wait_for_idle();
    assert_eq!(state().selected_date_millis(), Some(day_in_utc_millis(2020, 1, 15)));
    // Opened again, the grid marks the year chosen last.
    ui.on_node_with_text("January 2020").perform_click();
    ui.on_node_with_text("Navigate to year 2019").assert_is_not_selected();
    ui.on_node_with_text("Navigate to year 2020").assert_is_selected();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, yearRange
#[test]
fn the_grid_has_the_years_of_the_range() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2019, 1, 1)),
            year_range = 2018..=2023
        )
    });
    ui.on_node_with_text("January 2019").perform_click();
    for year in 2018..=2023 {
        ui.on_node_with_text(&format!("Navigate to year {year}")).assert_exists();
    }
    ui.on_node_with_text("Navigate to year 2017").assert_does_not_exist();
    ui.on_node_with_text("Navigate to year 2024").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, yearRange_minYearAfterCurrentYear
#[test]
fn a_range_after_today_shows_its_first_january() {
    let current_year = model().today().year;
    thread_local! { static YEAR: Cell<i32> = const { Cell::new(0) }; }
    YEAR.with(|cell| cell.set(current_year));
    let mut ui = picker(|| {
        let year = YEAR.with(Cell::get);
        remember_date_picker_state!(year_range = (year + 1)..=(year + 10))
    });
    ui.on_node_with_text(&format!("January {}", current_year + 1)).assert_is_displayed();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, monthsTraversal
#[test]
fn the_arrows_go_through_the_months() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2018, 1, 1))
        )
    });
    ui.on_node_with_text("January 2018").assert_exists();
    ui.on_node(next()).perform_click();
    ui.wait_for_idle();
    ui.on_node_with_text("February 2018").assert_exists();
    ui.on_node_with_text("January 2018").assert_does_not_exist();
    ui.on_node(previous()).perform_click();
    ui.wait_for_idle();
    ui.on_node_with_text("January 2018").assert_exists();
    ui.on_node_with_text("February 2018").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, monthsTraversalAtRangeEdges
#[test]
fn the_arrows_stop_at_the_ends_of_the_range() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2018, 1, 1)),
            year_range = 2018..=2018
        )
    });
    ui.on_node(next()).assert_is_enabled();
    ui.on_node(previous()).assert_is_not_enabled();
    for _ in 0..11 {
        ui.on_node(next()).perform_click();
        ui.on_node(previous()).assert_is_enabled();
    }
    ui.on_node(next()).assert_is_not_enabled();
    for _ in 0..11 {
        ui.on_node(previous()).perform_click();
        ui.on_node(next()).assert_is_enabled();
    }
    ui.on_node(previous()).assert_is_not_enabled();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, monthsTraversalInitialAtEndEdge
#[test]
fn the_arrows_follow_a_month_set_on_the_state() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2018, 12, 1)),
            year_range = 2018..=2018
        )
    });
    ui.on_node(next()).assert_is_not_enabled();
    ui.on_node(previous()).assert_is_enabled();
    ui.run_on_idle(|_| state().set_displayed_month_millis(day_in_utc_millis(2018, 1, 1)));
    ui.wait_for_idle();
    ui.on_node(next()).assert_is_enabled();
    ui.on_node(previous()).assert_is_not_enabled();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, monthsTraversal_withSelectableDatesValidator
#[test]
fn the_arrows_go_by_the_range_not_the_selectable_dates() {
    struct Only2019;
    impl SelectableDates for Only2019 {
        fn is_selectable_year(&self, year: i32) -> bool {
            year == 2019
        }
        fn is_selectable_date(&self, _utc_time_millis: i64) -> bool {
            false
        }
    }
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2018, 12, 1)),
            year_range = 2018..=2019,
            selectable_dates = remember(|| Rc::new(Only2019) as Rc<dyn SelectableDates>)
        )
    });
    ui.on_node(previous()).assert_is_enabled();
    ui.on_node(next()).assert_is_enabled();
    ui.on_node(next()).perform_click();
    ui.on_node_with_text("January 2019").assert_is_displayed();
    ui.on_node(previous()).assert_is_enabled();
    ui.on_node(next()).assert_is_enabled();
    ui.on_node(previous()).perform_click();
    ui.on_node_with_text("December 2018").assert_is_displayed();
    ui.on_node(previous()).assert_is_enabled();
    ui.on_node(next()).assert_is_enabled();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, monthsTraversal_acrossYearBoundary
#[test]
fn the_arrows_cross_a_year() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2018, 12, 1)),
            year_range = 2018..=2019
        )
    });
    ui.on_node_with_text("December 2018").assert_is_displayed();
    ui.on_node(next()).perform_click();
    ui.on_node_with_text("January 2019").assert_is_displayed();
    ui.on_node(previous()).assert_is_enabled();
    ui.on_node(next()).assert_is_enabled();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, monthsTraversal_switchingDisplayModesPreservesNavigationState
#[test]
fn switching_modes_keeps_where_the_arrows_are() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2018, 1, 1)),
            year_range = 2018..=2018
        )
    });
    ui.on_node(previous()).assert_is_not_enabled();
    ui.on_node(next()).assert_is_enabled();
    ui.on_node_with_content_description("Switch to text input mode").perform_click();
    ui.wait_for_idle();
    ui.on_node_with_content_description("Switch to calendar input mode").perform_click();
    ui.wait_for_idle();
    ui.on_node(previous()).assert_is_not_enabled();
    ui.on_node(next()).assert_is_enabled();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, switchToDateInput
#[test]
fn the_toggle_goes_to_the_input() {
    let mut ui = picker(|| remember_date_picker_state!());
    ui.on_node_with_content_description("Switch to text input mode").perform_click();
    ui.wait_for_idle();
    ui.on_node_with_text("Date").assert_is_displayed();
    ui.on_node(next()).assert_does_not_exist();
    ui.on_node(previous()).assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, datePicker_swipeLeft_goesToNextMonth
#[test]
fn a_swipe_goes_to_the_next_month() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            DatePicker!(
                remember_date_picker_state!(
                    initial_selected_date_millis = Some(day_in_utc_millis(2018, 1, 1)),
                    year_range = 2016..=2019
                ),
                modifier = Modifier.test_tag("datePicker")
            );
        });
    });
    ui.on_all_nodes(has_text_containing("Feb")).assert_count_equals(0);
    ui.on_node_with_tag("datePicker").perform_touch_input(|touch| {
        let center = touch.center();
        touch.swipe_left_with(center.x, touch.center_left().x, 200);
    });
    ui.wait_for_idle();
    ui.on_all_nodes(has_text_containing("Feb")).on_first().assert_is_displayed();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, state_initWithoutRemember
#[test]
fn a_state_made_by_hand_has_its_initial_values() {
    let state = DatePickerState::new(
        Locale::new("en-US"),
        Some(1649721600000),
        Some(1649721600000),
        DatePickerDefaults::YEAR_RANGE,
        DisplayMode::Picker,
        DatePickerDefaults::all_dates(),
    );
    assert_eq!(state.selected_date_millis(), Some(1649721600000));
    assert_eq!(state.displayed_month_millis(), model().get_month(2022, 4).start_utc_time_millis);
    assert_eq!(state.locale(), Locale::new("en-US"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, state_initWithSelectedDate
#[test]
fn the_displayed_month_is_the_selected_dates() {
    let _ui =
        picker(|| remember_date_picker_state!(initial_selected_date_millis = Some(1649721600000)));
    assert_eq!(state().selected_date_millis(), Some(1649721600000));
    assert_eq!(state().displayed_month_millis(), model().get_month(2022, 4).start_utc_time_millis);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, state_initWithSelectedDate_roundingToStartDay
#[test]
fn the_selected_date_rounds_down_to_its_day() {
    let _ui = picker(|| {
        remember_date_picker_state!(initial_selected_date_millis = Some(1649721600000 + 10000))
    });
    assert_eq!(state().selected_date_millis(), Some(1649721600000));
    assert_eq!(state().displayed_month_millis(), model().get_month(2022, 4).start_utc_time_millis);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, state_initWithSelectedDateAndNullMonth
#[test]
fn no_displayed_month_is_the_current_one() {
    let _ui = picker(|| {
        remember_date_picker_state!(
            initial_selected_date_millis = Some(1649721600000),
            initial_displayed_month_millis = Some(model().today().utc_time_millis)
        )
    });
    let model = model();
    assert_eq!(state().selected_date_millis(), Some(1649721600000));
    assert_eq!(
        state().displayed_month_millis(),
        model.get_month_of_millis(model.today().utc_time_millis).start_utc_time_millis
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, state_initWithNulls
#[test]
fn no_date_and_no_month_are_the_current_month() {
    let _ui = picker(|| {
        remember_date_picker_state!(
            initial_selected_date_millis = None,
            initial_displayed_month_millis = None
        )
    });
    let model = model();
    assert_eq!(state().selected_date_millis(), None);
    assert_eq!(
        state().displayed_month_millis(),
        model.get_month_of_millis(model.today().utc_time_millis).start_utc_time_millis
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, state_resetSelection
#[test]
fn resetting_the_selection_clears_the_headline() {
    let mut ui =
        picker(|| remember_date_picker_state!(initial_selected_date_millis = Some(1649721600000)));
    ui.on_node_with_text("Apr 12, 2022").assert_exists();
    assert_eq!(state().selected_date_millis(), Some(1649721600000));
    assert_eq!(state().displayed_month_millis(), model().get_month(2022, 4).start_utc_time_millis);
    state().set_selected_date_millis(None);
    assert_eq!(state().selected_date_millis(), None);
    ui.wait_for_idle();
    ui.on_node_with_text("Apr 12, 2022").assert_does_not_exist();
    ui.on_node_with_text("Selected date").assert_exists();
    assert_eq!(state().displayed_month_millis(), model().get_month(2022, 4).start_utc_time_millis);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, state_restoresDatePickerState
#[test]
fn a_state_is_saved_and_restored() {
    let mut rule = compose_test(|| {});
    let mut tester = StateRestorationTester::new();
    tester.set_content(&mut rule, || {
        let state = remember_date_picker_state!();
        STATE.with(|cell| cell.set(Some(state)));
    });
    let date = model().get_canonical_date(1649721600000);
    let month = model().get_month_of_date(&date);
    rule.run_on_idle(|_| {
        state().set_selected_date_millis(Some(date.utc_time_millis));
        state().set_displayed_month_millis(month.start_utc_time_millis);
    });
    STATE.with(|cell| cell.set(None));
    tester.emulate_save_and_restore(&mut rule);
    assert_eq!(state().selected_date_millis(), Some(1649721600000));
    assert_eq!(state().displayed_month_millis(), month.start_utc_time_millis);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, state_changeDisplayedMonth
#[test]
fn setting_the_displayed_month_moves_the_calendar() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2020, 1, 1))
        )
    });
    ui.on_node_with_text("January 2020").assert_exists();
    state().set_displayed_month_millis(day_in_utc_millis(2020, 7, 1));
    ui.wait_for_idle();
    ui.on_node_with_text("July 2020").assert_exists();
    ui.on_node(next()).perform_click();
    ui.wait_for_idle();
    ui.on_node_with_text("August 2020").assert_exists();
    ui.on_node(previous()).perform_click();
    ui.wait_for_idle();
    ui.on_node_with_text("July 2020").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, setSelection_outOfYearsBound
#[test]
fn a_selection_out_of_the_years_is_none() {
    let _ui = picker(|| remember_date_picker_state!(year_range = 2000..=2050));
    state().set_selected_date_millis(Some(day_in_utc_millis(1999, 5, 11)));
    assert_eq!(state().selected_date_millis(), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, initialDateOutOfBounds
#[test]
fn an_initial_date_out_of_the_years_is_none() {
    let _ui = picker(|| {
        remember_date_picker_state!(
            initial_selected_date_millis = Some(day_in_utc_millis(2051, 5, 11)),
            year_range = 2000..=2050
        )
    });
    assert_eq!(state().selected_date_millis(), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, initialDisplayedMonthOutObBounds
#[test]
fn an_initial_month_out_of_the_years_is_the_current_one() {
    let _ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(1999, 1, 1)),
            year_range = 2000..=2050
        )
    });
    let model = model();
    assert_eq!(
        state().displayed_month_millis(),
        model.get_month_of_date(&model.today()).start_utc_time_millis
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, defaultSemantics
#[test]
fn the_buttons_days_and_headline_read_as_they_should() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_selected_date_millis = Some(day_in_utc_millis(2010, 5, 11)),
            initial_displayed_month_millis = Some(day_in_utc_millis(2010, 5, 1))
        )
    });
    let full_date = "Tuesday, May 11, 2010";
    let button = || expect_value(&SemanticsProperties::ROLE, compose_ui::Role::Button);
    ui.on_node(next()).assert(button());
    ui.on_node(previous()).assert(button());
    ui.on_node_with_text("May 2010").assert(button());
    ui.on_node(has_text(full_date).and(has_click_action())).assert(button());
    ui.on_node_with_text("May 11, 2010")
        .assert_content_description_equals(&[&format!("Current selection: {full_date}")]);
}

/// A text measurer recording the color of each text it lays out.
struct RecordingColors {
    inner: compose_ui::platform::FixedTextMeasurer,
    log: Rc<RefCell<Vec<(String, Color)>>>,
}

impl compose_ui::platform::TextMeasurer for RecordingColors {
    fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        options: compose_ui::platform::TextLayoutOptions,
        constraints: compose_ui::Constraints,
        density: &Density,
    ) -> compose_ui::platform::ParagraphHandle {
        self.log.borrow_mut().push((text.to_string(), style.color));
        self.inner.layout(text, style, options, constraints, density)
    }
    fn metrics(
        &self,
        paragraph: compose_ui::platform::ParagraphHandle,
    ) -> compose_ui::platform::ParagraphMetrics {
        self.inner.metrics(paragraph)
    }
    fn hit_test(
        &self,
        paragraph: compose_ui::platform::ParagraphHandle,
        position: compose_ui::Offset,
    ) -> compose_ui::platform::TextPosition {
        self.inner.hit_test(paragraph, position)
    }
    fn cursor_rect(
        &self,
        paragraph: compose_ui::platform::ParagraphHandle,
        offset: usize,
    ) -> compose_ui::Rect {
        self.inner.cursor_rect(paragraph, offset)
    }
    fn text(&self, paragraph: compose_ui::platform::ParagraphHandle) -> &str {
        self.inner.text(paragraph)
    }
    fn release(&mut self, paragraph: compose_ui::platform::ParagraphHandle) {
        self.inner.release(paragraph)
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, customColorsSupersedeTypographyColors
#[test]
fn custom_colors_win_over_the_typographys() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut ui = compose_test_with(
        IntSize::new(400, 800),
        Density::default(),
        Box::new(RecordingColors {
            inner: compose_ui::platform::FixedTextMeasurer::new(),
            log: log.clone(),
        }),
        || {
            MaterialTheme!(
                typography = Typography {
                    body_large: TextStyle { color: Color::YELLOW, ..TextStyle::default() },
                    headline_large: TextStyle { color: Color::GREEN, ..TextStyle::default() },
                    ..Typography::default()
                },
                {
                    let state = remember_date_picker_state!(
                        initial_selected_date_millis = Some(day_in_utc_millis(2010, 5, 11)),
                        initial_displayed_month_millis = Some(day_in_utc_millis(2010, 5, 1))
                    );
                    DatePicker!(
                        state,
                        colors = DatePickerColors {
                            day_content_color: Color::BLUE,
                            selected_day_content_color: Color::RED,
                            headline_content_color: Color::YELLOW,
                            ..DatePickerDefaults::colors()
                        }
                    );
                }
            );
        },
    );
    ui.wait_for_idle();
    let color_of = |text: &str| {
        log.borrow().iter().rev().find(|(logged, _)| logged == text).map(|(_, color)| *color)
    };
    assert_eq!(color_of("11"), Some(Color::RED));
    assert_eq!(color_of("12"), Some(Color::BLUE));
    assert_eq!(color_of("May 11, 2010"), Some(Color::YELLOW));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, navigationContentColor_appliesToYearPickerMenuButtonIcon
#[test]
fn the_navigation_color_tints_the_year_buttons_icon() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            let state = remember_date_picker_state!(
                initial_displayed_month_millis = Some(day_in_utc_millis(2010, 5, 1))
            );
            DatePicker!(
                state,
                colors = DatePickerColors {
                    navigation_content_color: Color::RED,
                    ..DatePickerDefaults::colors()
                }
            );
        });
    });
    let bounds = ui
        .on_node_with_content_description_unmerged("Activate to change the year")
        .fetch_semantics_snapshot()
        .bounds;
    let red_inside = ui.host().display_list().ops.iter().any(|op| {
        matches!(op, compose_ui::platform::DrawOp::DrawPath { paint, .. } if paint.color == Color::RED)
    });
    assert!(red_inside, "nothing drawn in red around {bounds:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, yearGrid_keyboardNavigation
#[test]
fn tab_goes_through_the_years_and_the_buttons() {
    let mut ui = picker_in_dialog(LayoutDirection::Ltr);
    ui.wait_for_idle();
    ui.on_node_with_text("January 2010").request_focus();
    ui.on_node_with_text("January 2010").perform_key_input(|keys| keys.press_key(Key::Enter));
    ui.wait_for_idle();
    ui.on_node_with_text("Navigate to year 2010").assert_is_focused();
    ui.on_node_with_text("January 2010").perform_key_input(|keys| keys.press_key(Key::Tab));
    ui.on_node_with_text("OK").assert_is_focused();
    ui.on_node_with_text("OK").perform_key_input(|keys| {
        keys.with_key_down(Key::ShiftLeft, |keys| keys.press_key(Key::Tab));
    });
    ui.on_node_with_text("Cancel").perform_key_input(|keys| {
        keys.with_key_down(Key::ShiftLeft, |keys| keys.press_key(Key::Tab));
    });
    ui.on_node_with_text("Navigate to year 2010").assert_is_focused();
    ui.on_node_with_text("Navigate to year 2010").perform_key_input(|keys| {
        keys.with_key_down(Key::ShiftLeft, |keys| keys.press_key(Key::Tab));
    });
    ui.on_node_with_text("January 2010").assert_is_focused();
}

fn focused_day(text: &str) -> Matcher {
    has_text_containing(text)
}

fn first_day_keys(direction: LayoutDirection, forward: Key, backward: Key) {
    let mut ui = picker_in_dialog(direction);
    ui.wait_for_idle();
    ui.on_node(focused_day("January 1, 2010")).request_focus();
    ui.on_node(focused_day("January 1, 2010")).perform_key_input(|keys| keys.press_key(forward));
    ui.on_node(focused_day("January 2, 2010")).assert_is_focused();
    ui.on_node(focused_day("January 2, 2010")).perform_key_input(|keys| keys.press_key(backward));
    ui.on_node(focused_day("January 1, 2010")).assert_is_focused();
    ui.on_node(focused_day("January 1, 2010")).perform_key_input(|keys| keys.press_key(backward));
    ui.wait_for_idle();
    ui.on_node_with_text("January 2010").assert_does_not_exist();
    ui.on_node_with_text("December 2009").assert_exists();
    ui.on_node(focused_day("December 31, 2009")).assert_is_focused();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, firstDayOfMonth_keyboardBehavior
#[test]
fn the_arrows_go_from_a_months_first_day_to_the_month_before() {
    first_day_keys(LayoutDirection::Ltr, Key::DirectionRight, Key::DirectionLeft);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, firstDayOfMonth_keyboardBehavior_rtl
#[test]
fn the_arrows_go_from_a_months_first_day_to_the_month_before_in_rtl() {
    first_day_keys(LayoutDirection::Rtl, Key::DirectionLeft, Key::DirectionRight);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, lastDayOfMonth_keyboardBehavior
#[test]
fn tab_and_the_arrows_leave_a_months_last_day() {
    let mut ui = picker_in_dialog(LayoutDirection::Ltr);
    ui.wait_for_idle();
    let last = "January 31, 2010";
    ui.on_node(focused_day(last)).request_focus();
    ui.on_node(focused_day(last)).perform_key_input(|keys| keys.press_key(Key::Tab));
    ui.on_node_with_text("Cancel").assert_is_focused();
    ui.on_node_with_text("Cancel").perform_key_input(|keys| {
        keys.with_key_down(Key::ShiftLeft, |keys| keys.press_key(Key::Tab));
    });
    ui.on_node(focused_day(last)).assert_is_focused();
    ui.on_node(focused_day(last)).perform_key_input(|keys| keys.press_key(Key::DirectionRight));
    ui.wait_for_idle();
    ui.on_node_with_text("January 2010").assert_does_not_exist();
    ui.on_node_with_text("February 2010").assert_exists();
    ui.on_node(focused_day("February 1, 2010")).assert_is_focused();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, lastDayOfMonth_keyboardBehavior_rtl
#[test]
fn the_arrows_leave_a_months_last_day_in_rtl() {
    let mut ui = picker_in_dialog(LayoutDirection::Rtl);
    ui.wait_for_idle();
    let last = "January 31, 2010";
    ui.on_node(focused_day(last)).request_focus();
    ui.on_node(focused_day(last)).perform_key_input(|keys| keys.press_key(Key::DirectionRight));
    ui.on_node(focused_day("January 30, 2010")).assert_is_focused();
    ui.on_node(focused_day("January 30, 2010"))
        .perform_key_input(|keys| keys.press_key(Key::DirectionLeft));
    ui.on_node(focused_day(last)).assert_is_focused();
    ui.on_node(focused_day(last)).perform_key_input(|keys| keys.press_key(Key::DirectionLeft));
    ui.wait_for_idle();
    ui.on_node_with_text("January 2010").assert_does_not_exist();
    ui.on_node_with_text("February 2010").assert_exists();
    ui.on_node(focused_day("February 1, 2010")).assert_is_focused();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, cancelAndOkButtons_keyboardBehavior
#[test]
fn tab_goes_through_the_buttons_as_they_read() {
    let mut ui = picker_in_dialog(LayoutDirection::Ltr);
    ui.wait_for_idle();
    let last = "January 31, 2010";
    ui.on_node(focused_day(last)).request_focus();
    ui.on_node(focused_day(last)).perform_key_input(|keys| keys.press_key(Key::Tab));
    ui.on_node_with_text("Cancel").assert_is_focused();
    ui.on_node_with_text("Cancel").perform_key_input(|keys| keys.press_key(Key::Tab));
    ui.on_node_with_text("OK").assert_is_focused();
    ui.on_node_with_text("OK").perform_key_input(|keys| {
        keys.with_key_down(Key::ShiftLeft, |keys| keys.press_key(Key::Tab));
    });
    ui.on_node_with_text("Cancel").assert_is_focused();
    ui.on_node_with_text("Cancel").perform_key_input(|keys| {
        keys.with_key_down(Key::ShiftLeft, |keys| keys.press_key(Key::Tab));
    });
    ui.on_node(focused_day(last)).assert_is_focused();
}

fn through_the_days(direction: LayoutDirection, forward: Key) {
    thread_local! { static DIRECTION: Cell<LayoutDirection> = const { Cell::new(LayoutDirection::Ltr) }; }
    DIRECTION.with(|cell| cell.set(direction));
    let mut ui = compose_test(|| {
        MaterialTheme!({
            CompositionLocalProvider(
                LOCAL_LAYOUT_DIRECTION.provides(DIRECTION.with(Cell::get)),
                || {
                    DatePicker!(remember_date_picker_state!(
                        initial_selected_date_millis = Some(day_in_utc_millis(2010, 1, 11)),
                        initial_displayed_month_millis = Some(day_in_utc_millis(2010, 1, 1))
                    ));
                },
            );
        });
    });
    ui.wait_for_idle();
    ui.on_node(focused_day("January 1, 2010")).request_focus();
    for day in 1..=30 {
        ui.on_node(focused_day(&format!("January {day}, 2010")))
            .perform_key_input(|keys| keys.press_key(forward));
        ui.on_node(focused_day(&format!("January {}, 2010", day + 1))).assert_is_focused();
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, calendar_keyboardBehavior
#[test]
fn the_arrow_goes_through_all_the_days() {
    through_the_days(LayoutDirection::Ltr, Key::DirectionRight);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerTest.kt, calendar_keyboardBehavior_rtl
#[test]
fn the_arrow_goes_through_all_the_days_in_rtl() {
    through_the_days(LayoutDirection::Rtl, Key::DirectionLeft);
}

// ---- What Compose's screenshots show: places, sizes, colors ------------------------------------

fn bounds(ui: &mut ComposeTestRule, matcher: Matcher) -> compose_ui::Rect {
    ui.on_node(matcher).fetch_semantics_snapshot().bounds
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerScreenshotTest.kt, datePicker_initialMonth
#[test]
fn a_picker_is_its_header_its_navigation_its_weekdays_and_six_rows() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Box!({
                DatePicker!(
                    remember_date_picker_state!(
                        initial_displayed_month_millis = Some(day_in_utc_millis(2021, 1, 1))
                    ),
                    modifier = Modifier.test_tag("picker")
                );
            });
        });
    });
    let picker = ui.on_node_with_tag("picker").fetch_semantics_snapshot().bounds;
    assert_eq!(picker.width(), 400.0);
    assert_eq!(picker.height(), 120.0 + 56.0 + 48.0 + 288.0);
    // The week starts on Sunday in the US; January 2021 on its Friday: the sixth of seven cells.
    let sunday = bounds(&mut ui, has_content_description("Sunday"));
    let friday = bounds(&mut ui, has_content_description("Friday"));
    assert_eq!(sunday.top, 120.0 + 56.0);
    assert!(sunday.left < friday.left);
    let first = bounds(&mut ui, day("January 1, 2021"));
    assert_eq!((first.left, first.width()), (friday.left, 48.0));
    assert_eq!(first.top, sunday.bottom);
    // The 31st is on a sixth row.
    let last = bounds(&mut ui, day("January 31, 2021"));
    assert_eq!(last.left, sunday.left);
    assert_eq!(last.top, first.top + 5.0 * 48.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerScreenshotTest.kt, datePicker_customLocale
#[test]
fn a_regions_week_starts_on_its_first_day() {
    STATE.with(|cell| cell.set(None));
    let state = DatePickerState::new(
        Locale::new("en-GB"),
        None,
        Some(day_in_utc_millis(2021, 1, 1)),
        DatePickerDefaults::YEAR_RANGE,
        DisplayMode::Picker,
        DatePickerDefaults::all_dates(),
    );
    let state = Rc::new(state);
    let mut ui = compose_test(move || {
        let state = **state;
        MaterialTheme!({
            DatePicker!(state);
        });
    });
    let monday = bounds(&mut ui, has_content_description("Monday"));
    let sunday = bounds(&mut ui, has_content_description("Sunday"));
    assert!(monday.left < sunday.left);
    // Fridays are the fifth cell then.
    let first = bounds(&mut ui, has_text("Friday 1 January 2021").and(has_click_action()));
    assert_eq!(first.left, bounds(&mut ui, has_content_description("Friday")).left);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerScreenshotTest.kt, datePicker_arabicLocaleWithArabicNumerals
#[test]
fn a_locales_digits_are_on_the_days_and_years() {
    let state = DatePickerState::new(
        Locale::new("ar-u-nu-arab"),
        None,
        Some(day_in_utc_millis(2021, 1, 1)),
        DatePickerDefaults::YEAR_RANGE,
        DisplayMode::Picker,
        DatePickerDefaults::all_dates(),
    );
    let state = Rc::new(state);
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut ui = compose_test_with(
        IntSize::new(400, 800),
        Density::default(),
        Box::new(RecordingColors {
            inner: compose_ui::platform::FixedTextMeasurer::new(),
            log: log.clone(),
        }),
        move || {
            let state = **state;
            MaterialTheme!({
                DatePicker!(state);
            });
        },
    );
    ui.wait_for_idle();
    assert!(log.borrow().iter().any(|(text, _)| text == "٢٧"));
    assert!(log.borrow().iter().all(|(text, _)| text != "27"));
}

#[test]
fn no_slots_leave_no_header() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            DatePicker!(
                remember_date_picker_state!(
                    initial_displayed_month_millis = Some(day_in_utc_millis(2021, 1, 1))
                ),
                modifier = Modifier.test_tag("picker"),
                title = None::<fn()>,
                headline = None::<fn()>,
                show_mode_toggle = false
            );
        });
    });
    let picker = ui.on_node_with_tag("picker").fetch_semantics_snapshot().bounds;
    assert_eq!(picker.height(), 56.0 + 48.0 + 288.0);
    ui.on_node_with_text("Select date").assert_does_not_exist();
    ui.on_node_with_text("Selected date").assert_does_not_exist();
    ui.on_node_with_content_description("Switch to text input mode").assert_does_not_exist();
}

#[test]
fn slots_replace_the_title_and_headline() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            DatePicker!(
                remember_date_picker_state!(),
                title = Some(|| {
                    Text!("Departure");
                }),
                headline = Some(|| {
                    Text!("When?");
                })
            );
        });
    });
    ui.on_node_with_text("Departure").assert_exists();
    ui.on_node_with_text("When?").assert_exists();
    ui.on_node_with_text("Select date").assert_does_not_exist();
    ui.on_node_with_text("Selected date").assert_does_not_exist();
    ui.on_node_with_content_description("Switch to text input mode").assert_exists();
}

#[test]
fn a_formatter_of_other_skeletons_writes_the_dates() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            DatePicker!(
                remember_date_picker_state!(
                    initial_selected_date_millis = Some(day_in_utc_millis(2010, 5, 11))
                ),
                date_formatter = remember(|| {
                    DatePickerDefaults::date_formatter("MMMMy", "yMMMMEEEEd", "yMMMd")
                })
            );
        });
    });
    ui.on_node_with_text("May 2010").assert_exists();
    ui.on_node_with_text("Tuesday, May 11, 2010").assert_exists();
    ui.on_node(has_text("May 11, 2010").and(has_click_action())).assert_is_selected();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerScreenshotTest.kt, datePicker_todayMarker
#[test]
fn today_is_told_of_and_in_its_color() {
    let today = model().today();
    thread_local! { static TODAY: Cell<i64> = const { Cell::new(0) }; }
    TODAY.with(|cell| cell.set(today.utc_time_millis));
    let mut ui = picker(|| {
        remember_date_picker_state!(initial_displayed_month_millis = Some(TODAY.with(Cell::get)))
    });
    let full = compose_material3::calendar_model::format_with_skeleton(
        today.utc_time_millis,
        DatePickerDefaults::YEAR_MONTH_WEEKDAY_DAY_SKELETON,
        &Locale::new("en-US"),
    );
    ui.on_node(has_text(&format!("Today, {full}")).and(has_click_action()))
        .assert_is_not_selected();
    // Selected, it's still today, but without the outline.
    ui.on_node(has_text(&format!("Today, {full}")).and(has_click_action())).perform_click();
    ui.wait_for_idle();
    ui.on_node(has_text(&format!("Today, {full}")).and(has_click_action())).assert_is_selected();
}

type ColorsLog = Rc<RefCell<Vec<(String, Color)>>>;

/// A content and the colors its texts are laid out in, the last one last.
fn colors_log(content: fn()) -> (ComposeTestRule, ColorsLog) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let ui = compose_test_with(
        IntSize::new(400, 800),
        Density::default(),
        Box::new(RecordingColors {
            inner: compose_ui::platform::FixedTextMeasurer::new(),
            log: log.clone(),
        }),
        content,
    );
    (ui, log)
}

fn last_color(log: &ColorsLog, text: &str) -> Option<Color> {
    log.borrow().iter().rev().find(|(logged, _)| logged == text).map(|(_, color)| *color)
}

const DAY_CONTENT_COLOR: Color = Color::from_rgb(1, 0, 0);
const DISABLED_DAY_CONTENT_COLOR: Color = Color::from_rgb(2, 0, 0);
const SELECTED_DAY_CONTENT_COLOR: Color = Color::from_rgb(3, 0, 0);
const DISABLED_SELECTED_DAY_CONTENT_COLOR: Color = Color::from_rgb(4, 0, 0);
const TODAY_CONTENT_COLOR: Color = Color::from_rgb(5, 0, 0);
const YEAR_CONTENT_COLOR: Color = Color::from_rgb(6, 0, 0);
const DISABLED_YEAR_CONTENT_COLOR: Color = Color::from_rgb(7, 0, 0);
const CURRENT_YEAR_CONTENT_COLOR: Color = Color::from_rgb(8, 0, 0);
const SELECTED_YEAR_CONTENT_COLOR: Color = Color::from_rgb(9, 0, 0);
const DISABLED_SELECTED_YEAR_CONTENT_COLOR: Color = Color::from_rgb(10, 0, 0);

fn custom_colors() -> DatePickerColors {
    DatePickerColors {
        day_content_color: DAY_CONTENT_COLOR,
        disabled_day_content_color: DISABLED_DAY_CONTENT_COLOR,
        selected_day_content_color: SELECTED_DAY_CONTENT_COLOR,
        disabled_selected_day_content_color: DISABLED_SELECTED_DAY_CONTENT_COLOR,
        today_content_color: TODAY_CONTENT_COLOR,
        year_content_color: YEAR_CONTENT_COLOR,
        disabled_year_content_color: DISABLED_YEAR_CONTENT_COLOR,
        current_year_content_color: CURRENT_YEAR_CONTENT_COLOR,
        selected_year_content_color: SELECTED_YEAR_CONTENT_COLOR,
        disabled_selected_year_content_color: DISABLED_SELECTED_YEAR_CONTENT_COLOR,
        ..DatePickerDefaults::colors()
    }
}

struct OnlyEvenDays;
impl SelectableDates for OnlyEvenDays {
    fn is_selectable_date(&self, utc_time_millis: i64) -> bool {
        (utc_time_millis / 86_400_000) % 2 == 0
    }
    fn is_selectable_year(&self, year: i32) -> bool {
        year != 2011
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerScreenshotTest.kt, datePicker_invalidDateSelection
#[test]
fn the_days_take_their_states_colors() {
    let (mut ui, log) = colors_log(|| {
        MaterialTheme!({
            // The 11th of May 2010 is an even day from the epoch.
            let state = remember_date_picker_state!(
                initial_selected_date_millis = Some(day_in_utc_millis(2010, 5, 11)),
                selectable_dates = remember(|| Rc::new(OnlyEvenDays) as Rc<dyn SelectableDates>)
            );
            STATE.with(|cell| cell.set(Some(state)));
            DatePicker!(state, colors = custom_colors());
        });
    });
    ui.wait_for_idle();
    assert_eq!(day_in_utc_millis(2010, 5, 11) / 86_400_000 % 2, 0);
    assert_eq!(last_color(&log, "11"), Some(SELECTED_DAY_CONTENT_COLOR));
    assert_eq!(last_color(&log, "12"), Some(DISABLED_DAY_CONTENT_COLOR));
    assert_eq!(last_color(&log, "13"), Some(DAY_CONTENT_COLOR));
    // A selection that isn't selectable goes the disabled way.
    state().set_selected_date_millis(Some(day_in_utc_millis(2010, 5, 12)));
    ui.wait_for_idle();
    assert_eq!(last_color(&log, "12"), Some(DISABLED_SELECTED_DAY_CONTENT_COLOR));
    assert_eq!(last_color(&log, "11"), Some(DAY_CONTENT_COLOR));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerScreenshotTest.kt, datePicker_todayMarker
#[test]
fn today_takes_its_color() {
    let (mut ui, log) = colors_log(|| {
        MaterialTheme!({
            DatePicker!(remember_date_picker_state!(), colors = custom_colors());
        });
    });
    ui.wait_for_idle();
    let today = model().today().day_of_month.to_string();
    assert_eq!(last_color(&log, &today), Some(TODAY_CONTENT_COLOR));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerScreenshotTest.kt, datePicker_yearPicker
#[test]
fn the_years_take_their_states_colors() {
    let (mut ui, log) = colors_log(|| {
        MaterialTheme!({
            let state = remember_date_picker_state!(
                initial_displayed_month_millis = Some(day_in_utc_millis(2010, 1, 1)),
                selectable_dates = remember(|| Rc::new(OnlyEvenDays) as Rc<dyn SelectableDates>)
            );
            STATE.with(|cell| cell.set(Some(state)));
            DatePicker!(state, colors = custom_colors());
        });
    });
    ui.on_node_with_text("January 2010").perform_click();
    ui.wait_for_idle();
    assert_eq!(last_color(&log, "2010"), Some(SELECTED_YEAR_CONTENT_COLOR));
    assert_eq!(last_color(&log, "2012"), Some(YEAR_CONTENT_COLOR));
    assert_eq!(last_color(&log, "2011"), Some(DISABLED_YEAR_CONTENT_COLOR));
    // Three to a row, 72 by 36, 16 apart.
    let first = bounds(&mut ui, has_text("Navigate to year 2009"));
    let second = bounds(&mut ui, has_text("Navigate to year 2010"));
    let below = bounds(&mut ui, has_text("Navigate to year 2012"));
    assert_eq!((first.width(), first.height()), (72.0, 36.0));
    assert_eq!(first.top, second.top);
    assert_eq!(below.top, first.bottom + 16.0);
    assert_eq!(below.left, first.left);
    // The arrows make way.
    ui.on_node(next()).assert_does_not_exist();
    ui.on_node(previous()).assert_does_not_exist();
    ui.on_node_with_content_description_unmerged("Activate to change month and date")
        .assert_exists();
    // Choosing a year keeps the month and closes the grid.
    ui.on_node_with_text("Navigate to year 2012").perform_click();
    ui.wait_for_idle();
    ui.on_node_with_text("January 2012").assert_exists();
    ui.on_node_with_text("Navigate to year 2012").assert_does_not_exist();
    assert_eq!(state().displayed_month_millis(), day_in_utc_millis(2012, 1, 1));
}

#[test]
fn a_fast_swipe_goes_one_month_and_the_state_follows() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            let state = remember_date_picker_state!(
                initial_displayed_month_millis = Some(day_in_utc_millis(2018, 1, 1)),
                year_range = 2016..=2019
            );
            STATE.with(|cell| cell.set(Some(state)));
            DatePicker!(state, modifier = Modifier.test_tag("datePicker"));
        });
    });
    ui.on_node_with_tag("datePicker").perform_touch_input(|touch| {
        let center = touch.center();
        touch.swipe_left_with(center.x + 150.0, center.x - 150.0, 30);
    });
    ui.wait_for_idle();
    ui.on_node_with_text("February 2018").assert_exists();
    assert_eq!(state().displayed_month_millis(), day_in_utc_millis(2018, 2, 1));
    assert!(ui.on_all_nodes(day("March 1, 2018")).fetch_semantics_nodes().is_empty());
}

#[test]
fn a_month_out_of_the_years_is_ignored() {
    let _ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2018, 1, 1)),
            year_range = 2016..=2019
        )
    });
    state().set_displayed_month_millis(day_in_utc_millis(2020, 1, 1));
    assert_eq!(state().displayed_month_millis(), day_in_utc_millis(2018, 1, 1));
}

#[test]
fn switching_modes_shows_the_selected_dates_month() {
    let _ui = picker(|| {
        remember_date_picker_state!(
            initial_selected_date_millis = Some(day_in_utc_millis(2018, 5, 11)),
            initial_displayed_month_millis = Some(day_in_utc_millis(2018, 7, 1))
        )
    });
    assert_eq!(state().displayed_month_millis(), day_in_utc_millis(2018, 7, 1));
    state().set_display_mode(DisplayMode::Input);
    assert_eq!(state().display_mode(), DisplayMode::Input);
    assert_eq!(state().displayed_month_millis(), day_in_utc_millis(2018, 5, 1));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerScreenshotTest.kt, datePicker_inDialog
#[test]
fn a_dialog_is_360_wide_its_buttons_at_the_end() {
    let mut ui = picker_in_dialog(LayoutDirection::Ltr);
    ui.wait_for_idle();
    let cancel = bounds(&mut ui, has_text("Cancel"));
    let ok = bounds(&mut ui, has_text("OK"));
    assert!(cancel.right <= ok.left);
    assert_eq!(cancel.top, ok.top);
    // In a 400 wide window, the dialog is centered: 20 off each side, the buttons 6 in.
    assert_eq!(ok.right, 400.0 - 20.0 - 6.0);
    let last_row = bounds(&mut ui, day("January 31, 2010"));
    assert!(last_row.bottom <= ok.top);
}

#[test]
fn a_blocked_years_days_are_disabled() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2019, 1, 1)),
            selectable_dates = remember(|| Rc::new(NoYears) as Rc<dyn SelectableDates>)
        )
    });
    ui.on_node(day("27")).assert_is_not_enabled();
}

#[test]
fn a_year_picked_keeps_the_month() {
    let mut ui = picker(|| {
        remember_date_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2019, 3, 1))
        )
    });
    ui.on_node_with_text("March 2019").perform_click();
    ui.wait_for_idle();
    ui.on_node_with_text("Navigate to year 2020").perform_click();
    ui.wait_for_idle();
    ui.on_node_with_text("March 2020").assert_exists();
    assert_eq!(state().displayed_month_millis(), day_in_utc_millis(2020, 3, 1));
}

#[test]
fn shift_tab_from_a_months_first_day_goes_to_the_next_month_button() {
    let mut ui = picker_in_dialog(LayoutDirection::Ltr);
    ui.wait_for_idle();
    ui.on_node(focused_day("January 1, 2010")).request_focus();
    ui.on_node(focused_day("January 1, 2010")).perform_key_input(|keys| {
        keys.with_key_down(Key::ShiftLeft, |keys| keys.press_key(Key::Tab));
    });
    ui.on_node(next()).assert_is_focused();
}

#[test]
fn the_current_year_takes_its_color() {
    let (mut ui, log) = colors_log(|| {
        MaterialTheme!({
            let today = model().today();
            let state = remember_date_picker_state!(
                initial_displayed_month_millis = Some(day_in_utc_millis(today.year + 1, 1, 1))
            );
            DatePicker!(state, colors = custom_colors());
        });
    });
    let today = model().today();
    ui.on_node_with_text(&format!("January {}", today.year + 1)).perform_click();
    ui.wait_for_idle();
    assert_eq!(last_color(&log, &today.year.to_string()), Some(CURRENT_YEAR_CONTENT_COLOR));
}

#[test]
fn the_header_ends_in_a_divider_when_it_has_something() {
    fn divider_lines(ui: &mut ComposeTestRule) -> usize {
        let ops = ui.host().display_list().ops.clone();
        ops.iter()
            .filter(|op| {
                matches!(
                    op,
                    compose_ui::platform::DrawOp::DrawLine(_, _, paint)
                        if paint.color == Color::MAGENTA
                )
            })
            .count()
    }
    let mut ui = compose_test(|| {
        MaterialTheme!({
            DatePicker!(
                remember_date_picker_state!(),
                colors = DatePickerColors {
                    divider_color: Color::MAGENTA,
                    ..DatePickerDefaults::colors()
                }
            );
        });
    });
    assert_eq!(divider_lines(&mut ui), 1);
    let mut ui = compose_test(|| {
        MaterialTheme!({
            DatePicker!(
                remember_date_picker_state!(),
                colors = DatePickerColors {
                    divider_color: Color::MAGENTA,
                    ..DatePickerDefaults::colors()
                },
                title = None::<fn()>,
                headline = None::<fn()>,
                show_mode_toggle = false
            );
        });
    });
    assert_eq!(divider_lines(&mut ui), 0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerScreenshotTest.kt, datePicker_noMinimumInteractiveSize
#[test]
fn without_a_touch_target_the_cells_are_40_wide() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            CompositionLocalProvider(
                LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.provides(compose_ui::Dp::UNSPECIFIED),
                || {
                    DatePicker!(remember_date_picker_state!(
                        initial_displayed_month_millis = Some(day_in_utc_millis(2021, 1, 1))
                    ));
                },
            );
        });
    });
    let sunday = bounds(&mut ui, has_content_description("Sunday"));
    assert_eq!((sunday.width(), sunday.height()), (40.0, 40.0));
    let first = bounds(&mut ui, day("January 1, 2021"));
    assert_eq!((first.width(), first.height()), (40.0, 40.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DatePickerScreenshotTest.kt, datePicker_arabicLocaleWithLatinNumerals
#[test]
fn latin_numerals_are_asked_for_in_arabic() {
    let state = DatePickerState::new(
        Locale::new("ar-u-nu-latn"),
        None,
        Some(day_in_utc_millis(2021, 1, 1)),
        DatePickerDefaults::YEAR_RANGE,
        DisplayMode::Picker,
        DatePickerDefaults::all_dates(),
    );
    let state = Rc::new(state);
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut ui = compose_test_with(
        IntSize::new(400, 800),
        Density::default(),
        Box::new(RecordingColors {
            inner: compose_ui::platform::FixedTextMeasurer::new(),
            log: log.clone(),
        }),
        move || {
            let state = **state;
            MaterialTheme!({
                DatePicker!(state);
            });
        },
    );
    ui.wait_for_idle();
    assert!(log.borrow().iter().any(|(text, _)| text == "27"));
    assert!(log.borrow().iter().all(|(text, _)| text != "٢٧"));
}

#[test]
fn tab_from_the_confirm_button_leaves_the_buttons() {
    let mut ui = picker_in_dialog(LayoutDirection::Ltr);
    ui.wait_for_idle();
    ui.on_node_with_text("OK").request_focus();
    ui.on_node_with_text("OK").perform_key_input(|keys| keys.press_key(Key::Tab));
    ui.on_node_with_text("Cancel").assert_is_not_focused();
    ui.on_node_with_text("OK").assert_is_not_focused();
}
