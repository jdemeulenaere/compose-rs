//! Compose's `DateRangePickerTest`, at density 1 in a 400 by 800 window. Its `java.time` overloads
//! have nothing to adapt. The range's background is `DateRangePickerScreenshotTest`'s, as the
//! display list's rectangles.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_material3::calendar_model::CalendarModel;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{
    Color, Density, IntSize, Key, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Locale, Modifier,
    Offset, TextStyle, UnitExt,
};

use crate::date_picker::RecordingColors;

const MILLIS_IN_A_DAY: i64 = 86_400_000;
const APRIL_12_2022: i64 = 1_649_721_600_000;

fn model() -> CalendarModel {
    CalendarModel::new(Locale::new("en-US"))
}

fn day_in_utc_millis(year: i32, month: i32, day: i32) -> i64 {
    model().get_month(year, month).start_utc_time_millis + i64::from(day - 1) * MILLIS_IN_A_DAY
}

thread_local! {
    static STATE: Cell<Option<DateRangePickerState>> = const { Cell::new(None) };
}

fn state() -> DateRangePickerState {
    STATE.with(Cell::get).unwrap()
}

fn keep(make: fn() -> DateRangePickerState) -> ComposeTestRule {
    compose_test(move || {
        MaterialTheme!({
            STATE.with(|cell| cell.set(Some(make())));
        });
    })
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

fn day(text: &str) -> Matcher {
    has_text_containing(text).and(has_click_action())
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, state_initWithoutRemember
#[test]
fn a_state_made_outside_a_composition_holds_its_dates() {
    let state = DateRangePickerState::new(
        Locale::new("en-US"),
        Some(APRIL_12_2022),
        Some(APRIL_12_2022 + MILLIS_IN_A_DAY),
        None,
        DatePickerDefaults::YEAR_RANGE,
        DisplayMode::Picker,
        DatePickerDefaults::all_dates(),
    );
    assert_eq!(state.selected_start_date_millis(), Some(APRIL_12_2022));
    assert_eq!(state.selected_end_date_millis(), Some(APRIL_12_2022 + MILLIS_IN_A_DAY));
    assert_eq!(state.displayed_month_millis(), model().get_month(2022, 4).start_utc_time_millis);
    assert_eq!(state.locale(), Locale::new("en-US"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, state_initWithSelectedDates
#[test]
fn the_remembered_state_starts_on_the_start_dates_month() {
    let _ui = keep(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(APRIL_12_2022),
            initial_selected_end_date_millis = Some(APRIL_12_2022 + MILLIS_IN_A_DAY)
        )
    });
    assert_eq!(state().selected_start_date_millis(), Some(APRIL_12_2022));
    assert_eq!(state().selected_end_date_millis(), Some(APRIL_12_2022 + MILLIS_IN_A_DAY));
    assert_eq!(state().displayed_month_millis(), model().get_month(2022, 4).start_utc_time_millis);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, state_initWithSelectedDates_roundingToUtcMidnight
#[test]
fn the_initial_dates_round_down_to_their_days() {
    let _ui = keep(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(APRIL_12_2022 + 10_000),
            initial_selected_end_date_millis = Some(APRIL_12_2022 + MILLIS_IN_A_DAY + 10_000)
        )
    });
    assert_eq!(state().selected_start_date_millis(), Some(APRIL_12_2022));
    assert_eq!(state().selected_end_date_millis(), Some(APRIL_12_2022 + MILLIS_IN_A_DAY));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, state_initWithEndDateOnly
#[test]
fn an_end_without_a_start_selects_nothing() {
    let _ui = keep(|| {
        remember_date_range_picker_state!(initial_selected_end_date_millis = Some(APRIL_12_2022))
    });
    assert_eq!(state().selected_start_date_millis(), None);
    assert_eq!(state().selected_end_date_millis(), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, state_initWithEndDateBeforeStartDate
#[test]
fn an_initial_end_before_its_start_selects_nothing() {
    let _ui = keep(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(APRIL_12_2022),
            initial_selected_end_date_millis = Some(APRIL_12_2022 - MILLIS_IN_A_DAY)
        )
    });
    assert_eq!(state().selected_start_date_millis(), None);
    assert_eq!(state().selected_end_date_millis(), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, state_initWithEqualStartAndEndDates
#[test]
fn equal_initial_dates_are_a_day_long_range() {
    let _ui = keep(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(APRIL_12_2022),
            initial_selected_end_date_millis = Some(APRIL_12_2022)
        )
    });
    assert_eq!(state().selected_start_date_millis(), Some(APRIL_12_2022));
    assert_eq!(state().selected_end_date_millis(), Some(APRIL_12_2022));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, initialStartDateOutOfBounds
#[test]
fn a_start_out_of_the_years_selects_nothing() {
    let _ui = keep(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(day_in_utc_millis(1999, 5, 11)),
            initial_selected_end_date_millis = Some(day_in_utc_millis(2020, 5, 12)),
            year_range = 2000..=2050
        )
    });
    assert_eq!(state().selected_start_date_millis(), None);
    assert_eq!(state().selected_end_date_millis(), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, initialEndDateOutOfBounds
#[test]
fn an_end_out_of_the_years_keeps_the_start() {
    let _ui = keep(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(day_in_utc_millis(2020, 5, 11)),
            initial_selected_end_date_millis = Some(day_in_utc_millis(2051, 5, 12)),
            year_range = 2000..=2050
        )
    });
    assert_eq!(state().selected_start_date_millis(), Some(day_in_utc_millis(2020, 5, 11)));
    assert_eq!(state().selected_end_date_millis(), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, datesSelection
#[test]
fn two_clicks_select_a_range() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2019, 1, 1))
        )
    });
    ui.on_node_with_text_unmerged("Start date").assert_exists();
    ui.on_node_with_text_unmerged("End date").assert_exists();

    ui.on_all_nodes(day("10")).on_first().assert_is_not_selected();
    ui.on_all_nodes(day("10")).on_first().perform_click();
    ui.wait_for_idle();
    assert_eq!(state().selected_start_date_millis(), Some(day_in_utc_millis(2019, 1, 10)));
    assert_eq!(state().selected_end_date_millis(), None);
    ui.on_node_with_text_unmerged("Start date").assert_does_not_exist();
    ui.on_node_with_text_unmerged("Jan 10, 2019").assert_exists();
    ui.on_node_with_text_unmerged("End date").assert_exists();
    ui.on_all_nodes(day("10")).on_first().assert_is_selected();

    ui.on_all_nodes(day("14")).on_first().assert_is_not_selected();
    ui.on_all_nodes(day("14")).on_first().perform_click();
    ui.wait_for_idle();
    assert_eq!(state().selected_end_date_millis(), Some(day_in_utc_millis(2019, 1, 14)));
    ui.on_node_with_text_unmerged("End date").assert_does_not_exist();
    ui.on_node_with_text_unmerged("Jan 10, 2019").assert_exists();
    ui.on_node_with_text_unmerged("Jan 14, 2019").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, dateSelectionStartReset
#[test]
fn a_date_before_the_start_is_the_new_start() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2019, 3, 1))
        )
    });
    ui.on_all_nodes(day("15")).on_first().perform_click();
    ui.wait_for_idle();
    assert_eq!(state().selected_start_date_millis(), Some(day_in_utc_millis(2019, 3, 15)));
    assert_eq!(state().selected_end_date_millis(), None);
    ui.on_all_nodes(day("12")).on_first().perform_click();
    ui.wait_for_idle();
    assert_eq!(state().selected_start_date_millis(), Some(day_in_utc_millis(2019, 3, 12)));
    assert_eq!(state().selected_end_date_millis(), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, dateSelection_sameDateForStartAndEnd
#[test]
fn the_same_day_twice_is_a_range_and_a_third_time_a_new_start() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2019, 3, 1))
        )
    });
    let the_15th = day_in_utc_millis(2019, 3, 15);
    ui.on_all_nodes(day("15")).on_first().perform_click();
    ui.wait_for_idle();
    ui.on_all_nodes(day("15")).on_first().perform_click();
    ui.wait_for_idle();
    assert_eq!(state().selected_start_date_millis(), Some(the_15th));
    assert_eq!(state().selected_end_date_millis(), Some(the_15th));
    ui.on_all_nodes(day("15")).on_first().perform_click();
    ui.wait_for_idle();
    assert_eq!(state().selected_start_date_millis(), Some(the_15th));
    assert_eq!(state().selected_end_date_millis(), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, state_resetSelections
#[test]
fn no_dates_bring_the_placeholders_back() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(APRIL_12_2022 + 10_000),
            initial_selected_end_date_millis = Some(APRIL_12_2022 + MILLIS_IN_A_DAY)
        )
    });
    ui.on_node_with_text_unmerged("Apr 12, 2022").assert_exists();
    ui.on_node_with_text_unmerged("Apr 13, 2022").assert_exists();
    assert_eq!(state().selected_start_date_millis(), Some(APRIL_12_2022));

    state().set_selection(None, None);
    assert_eq!(state().selected_start_date_millis(), None);
    assert_eq!(state().selected_end_date_millis(), None);
    ui.wait_for_idle();
    ui.on_node_with_text_unmerged("Apr 12, 2022").assert_does_not_exist();
    ui.on_node_with_text_unmerged("Apr 13, 2022").assert_does_not_exist();
    ui.on_node_with_text_unmerged("Start date").assert_exists();
    ui.on_node_with_text_unmerged("End date").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, setSelection_outOfYearsBound
#[test]
fn a_start_set_out_of_the_years_is_none() {
    let _ui = keep(|| remember_date_range_picker_state!(year_range = 2000..=2050));
    state().set_selection(Some(day_in_utc_millis(1999, 5, 11)), None);
    assert_eq!(state().selected_start_date_millis(), None);
    assert_eq!(state().selected_end_date_millis(), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, setSelection_endBeforeStart
#[test]
fn an_end_set_before_its_start_selects_nothing() {
    let _ui = keep(|| remember_date_range_picker_state!());
    state().set_selection(
        Some(day_in_utc_millis(2000, 10, 10)),
        Some(day_in_utc_millis(1998, 10, 10)),
    );
    assert_eq!(state().selected_start_date_millis(), None);
    assert_eq!(state().selected_end_date_millis(), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, state_restoresDatePickerState
#[test]
fn a_range_state_is_saved_and_restored() {
    let mut rule = compose_test(|| {});
    let mut tester = StateRestorationTester::new();
    tester.set_content(&mut rule, || {
        STATE.with(|cell| cell.set(Some(remember_date_range_picker_state!())));
    });
    let month = model().get_month(2022, 4).start_utc_time_millis;
    rule.run_on_idle(|_| {
        state().set_selection(Some(APRIL_12_2022), Some(APRIL_12_2022 + MILLIS_IN_A_DAY));
        state().set_displayed_month_millis(month);
    });
    STATE.with(|cell| cell.set(None));
    tester.emulate_save_and_restore(&mut rule);
    assert_eq!(state().selected_start_date_millis(), Some(APRIL_12_2022));
    assert_eq!(state().selected_end_date_millis(), Some(APRIL_12_2022 + MILLIS_IN_A_DAY));
    assert_eq!(state().displayed_month_millis(), month);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, state_changeDisplayedMonth
#[test]
fn setting_the_displayed_month_moves_the_list() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2020, 1, 1))
        )
    });
    ui.on_node_with_text("January 2020").assert_exists();
    state().set_displayed_month_millis(day_in_utc_millis(2020, 7, 1));
    ui.wait_for_idle();
    ui.on_node_with_text("July 2020").assert_exists();
    assert_eq!(state().displayed_month_millis(), day_in_utc_millis(2020, 7, 1));
}

struct NoDates;
impl SelectableDates for NoDates {
    fn is_selectable_date(&self, _utc_time_millis: i64) -> bool {
        false
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, selectableDates_updatedSelection
#[test]
fn new_selectable_dates_disable_the_days_shown() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            let disabled = remember(|| mutable_state_of(false));
            let none = remember(|| Rc::new(NoDates) as Rc<dyn SelectableDates>);
            let dates = if disabled.get() { none } else { DatePickerDefaults::all_dates() };
            Column!(|scope| {
                let state = remember_date_range_picker_state!(
                    initial_displayed_month_millis = Some(day_in_utc_millis(2019, 1, 1)),
                    selectable_dates = dates.clone()
                );
                DateRangePicker!(state, modifier = Modifier.weight(scope, 1.0));
                Button!(move || disabled.set(true), modifier = Modifier.test_tag("disable"), {
                    Text!("Disable selection");
                });
            });
        });
    });
    ui.on_all_nodes(day("27")).assert_all(is_enabled());
    ui.on_node_with_tag("disable").perform_click();
    ui.wait_for_idle();
    ui.on_all_nodes(day("27")).assert_all(is_not_enabled());
}

thread_local! {
    static FIRST_YEAR: Cell<i32> = const { Cell::new(0) };
}

fn next_year() -> i32 {
    let year = model().get_month_of_date(&model().today()).year + 1;
    FIRST_YEAR.with(|first| first.set(year));
    year
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, yearRange_minYearAfterCurrentYear
#[test]
fn years_after_this_one_start_at_their_january() {
    let year = next_year();
    let mut ui = compose_test(|| {
        MaterialTheme!({
            let first = FIRST_YEAR.with(Cell::get);
            DateRangePicker!(remember_date_range_picker_state!(year_range = first..=first + 9));
        });
    });
    ui.on_node_with_text(&format!("January {year}")).assert_is_displayed();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, dateRangePicker_keyboardNavigation
#[test]
fn tab_leaves_the_months_both_ways() {
    let year = next_year();
    let mut ui = compose_test(|| {
        MaterialTheme!({
            let first = FIRST_YEAR.with(Cell::get);
            Column!(modifier = Modifier.fill_max_size(), |scope| {
                Box!(modifier = Modifier.test_tag("above").size(10.dp()).focusable());
                DateRangePicker!(
                    remember_date_range_picker_state!(year_range = first..=first + 9),
                    modifier = Modifier.weight(scope, 1.0),
                    show_mode_toggle = false
                );
                Box!(modifier = Modifier.test_tag("below").size(10.dp()).focusable());
            });
        });
    });
    let first_day = format!("January 1, {year}");
    ui.on_node_with_tag("above").request_focus();
    ui.on_node_with_tag("above").perform_key_input(|keys| keys.press_key(Key::Tab));
    ui.on_node(has_text_containing(&first_day)).assert_is_focused();
    ui.on_node(has_text_containing(&first_day)).perform_key_input(|keys| keys.press_key(Key::Tab));
    ui.on_node_with_tag("below").assert_is_focused();
    ui.on_node_with_tag("below").perform_key_input(|keys| {
        keys.with_key_down(Key::ShiftLeft, |keys| keys.press_key(Key::Tab));
    });
    ui.on_node_with_tag("below").assert_is_not_focused();
    ui.on_node_with_tag("above").assert_is_not_focused();

    let a_day = format!("February 18, {year}");
    ui.on_node(has_text_containing(&a_day)).request_focus();
    ui.on_node(has_text_containing(&a_day)).perform_key_input(|keys| {
        keys.with_key_down(Key::ShiftLeft, |keys| keys.press_key(Key::Tab));
    });
    ui.on_node_with_tag("above").assert_is_focused();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerTest.kt, customColorsSupersedeTypographyColors
#[test]
fn the_ranges_colors_win_over_the_typographys() {
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
                    title_large: TextStyle { color: Color::GREEN, ..TextStyle::default() },
                    ..Typography::default()
                },
                {
                    DateRangePicker!(
                        remember_date_range_picker_state!(
                            initial_selected_start_date_millis =
                                Some(day_in_utc_millis(2021, 3, 6)),
                            initial_selected_end_date_millis = Some(day_in_utc_millis(2021, 3, 10)),
                            initial_displayed_month_millis = Some(day_in_utc_millis(2021, 3, 1))
                        ),
                        colors = DatePickerColors {
                            day_content_color: Color::BLUE,
                            selected_day_content_color: Color::RED,
                            headline_content_color: Color::YELLOW,
                            day_in_selection_range_content_color: Color::GREEN,
                            ..DatePickerDefaults::colors()
                        }
                    );
                }
            );
        },
    );
    ui.wait_for_idle();
    // March is laid out first, then the April under it.
    let color_of = |text: &str| {
        log.borrow().iter().find(|(logged, _)| logged == text).map(|(_, color)| *color)
    };
    assert_eq!(color_of("6"), Some(Color::RED));
    assert_eq!(color_of("10"), Some(Color::RED));
    assert_eq!(color_of("7"), Some(Color::GREEN));
    assert_eq!(color_of("11"), Some(Color::BLUE));
    assert_eq!(color_of("Mar 6, 2021"), Some(Color::YELLOW));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerScreenshotTest.kt, dateRangePicker_initialMonthAndSelection
#[test]
fn the_days_of_a_range_are_told_of_what_they_are() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(day_in_utc_millis(2021, 3, 6)),
            initial_selected_end_date_millis = Some(day_in_utc_millis(2021, 3, 10))
        )
    });
    ui.on_node(day("Start date, Saturday, March 6, 2021")).assert_is_selected();
    ui.on_node(day("In range, Sunday, March 7, 2021")).assert_is_not_selected();
    ui.on_node(day("End date, Wednesday, March 10, 2021")).assert_is_selected();
    ui.on_node(has_text("Thursday, March 11, 2021").and(has_click_action()))
        .assert_is_not_selected();
}

const RANGE_COLOR: Color = Color::from_rgb(1, 2, 3);

thread_local! {
    static RANGE: Cell<(Option<i64>, Option<i64>)> = const { Cell::new((None, None)) };
    static DIRECTION: Cell<LayoutDirection> = const { Cell::new(LayoutDirection::Ltr) };
}

/// The range's rectangles, in the window, as left, top, right, bottom: a picker 360 wide, so the
/// days' cells are 48 by 48, no space between them.
fn range_rectangles(
    start: Option<i64>,
    end: Option<i64>,
    direction: LayoutDirection,
) -> Vec<[f32; 4]> {
    RANGE.with(|range| range.set((start, end)));
    DIRECTION.with(|cell| cell.set(direction));
    let mut ui = compose_test(|| {
        let (start, end) = RANGE.with(Cell::get);
        CompositionLocalProvider(
            LOCAL_LAYOUT_DIRECTION.provides(DIRECTION.with(Cell::get)),
            || {
                MaterialTheme!({
                    DateRangePicker!(
                        remember_date_range_picker_state!(
                            initial_selected_start_date_millis = start,
                            initial_selected_end_date_millis = end,
                            initial_displayed_month_millis = Some(day_in_utc_millis(2021, 3, 1))
                        ),
                        modifier = Modifier.width(360.dp()),
                        colors = DatePickerColors {
                            day_in_selection_range_container_color: RANGE_COLOR,
                            ..DatePickerDefaults::colors()
                        }
                    );
                });
            },
        );
    });
    ui.wait_for_idle();
    let mut stack = vec![Offset::ZERO];
    let mut rectangles = Vec::new();
    for op in &ui.host().display_list().ops {
        let at = *stack.last().expect("a translation");
        match op {
            DrawOp::Save | DrawOp::SaveLayerAlpha { .. } => stack.push(at),
            DrawOp::Restore => {
                stack.pop();
            }
            DrawOp::Translate(by) => *stack.last_mut().expect("a translation") = at + *by,
            DrawOp::DrawRect(rect, paint) if paint.color == RANGE_COLOR => rectangles.push([
                rect.left + at.x,
                rect.top + at.y,
                rect.right + at.x,
                rect.bottom + at.y,
            ]),
            _ => {}
        }
    }
    rectangles
}

/// The rectangles of the first month drawn, against it: the first's top is 4 into its row, and the
/// month is 12 from the picker's side.
fn in_the_month(rectangles: &[[f32; 4]], count: usize) -> Vec<[f32; 4]> {
    let top = rectangles[0][1] - 4.0;
    let mut placed: Vec<[f32; 4]> = rectangles[..count]
        .iter()
        .map(|[left, up, right, down]| [left - 12.0, up - top, right - 12.0, down - top])
        .collect();
    placed.sort_by(|a, b| a[1].total_cmp(&b[1]));
    placed
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerScreenshotTest.kt, dateRangePicker_initialMonthAndSelection
#[test]
fn a_range_in_one_row_goes_from_center_to_center() {
    // March 2021 starts on a Monday, in the second cell: the 8th to the 10th are on row 1.
    let rectangles = range_rectangles(
        Some(day_in_utc_millis(2021, 3, 8)),
        Some(day_in_utc_millis(2021, 3, 10)),
        LayoutDirection::Ltr,
    );
    assert_eq!(rectangles.len(), 1, "{rectangles:?}");
    assert_eq!(rectangles[0][0] - 12.0, 72.0);
    assert_eq!(rectangles[0][2] - 12.0, 168.0);
    assert_eq!(rectangles[0][3] - rectangles[0][1], 40.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerScreenshotTest.kt, dateRangePicker_initialMonthAndSelection
#[test]
fn a_range_over_rows_fills_the_rows_between() {
    let rectangles = range_rectangles(
        Some(day_in_utc_millis(2021, 3, 6)),
        Some(day_in_utc_millis(2021, 3, 20)),
        LayoutDirection::Ltr,
    );
    assert_eq!(
        in_the_month(&rectangles, 3),
        vec![[312.0, 4.0, 336.0, 44.0], [0.0, 52.0, 336.0, 92.0], [0.0, 100.0, 312.0, 140.0],]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerScreenshotTest.kt, dateRangePicker_selectionSpanningMonths
#[test]
fn a_range_past_a_month_goes_to_its_first_and_last_cells() {
    let rectangles = range_rectangles(
        Some(day_in_utc_millis(2021, 2, 20)),
        Some(day_in_utc_millis(2021, 4, 5)),
        LayoutDirection::Ltr,
    );
    // March first: its 1st in the second cell of row 0, its 31st in the fourth of row 4.
    assert_eq!(
        in_the_month(&rectangles, 5),
        vec![
            [48.0, 4.0, 336.0, 44.0],
            [0.0, 52.0, 336.0, 92.0],
            [0.0, 100.0, 336.0, 140.0],
            [0.0, 148.0, 336.0, 188.0],
            [0.0, 196.0, 192.0, 236.0],
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerScreenshotTest.kt, dateRangePicker_selectionSpanningMonths_rtl
#[test]
fn a_right_to_left_range_is_mirrored() {
    let rectangles = range_rectangles(
        Some(day_in_utc_millis(2021, 3, 6)),
        Some(day_in_utc_millis(2021, 3, 20)),
        LayoutDirection::Rtl,
    );
    assert_eq!(
        in_the_month(&rectangles, 3),
        vec![[0.0, 4.0, 24.0, 44.0], [0.0, 52.0, 336.0, 92.0], [24.0, 100.0, 336.0, 140.0],]
    );
}

#[test]
fn a_start_alone_is_not_yet_a_range() {
    let rectangles =
        range_rectangles(Some(day_in_utc_millis(2021, 3, 6)), None, LayoutDirection::Ltr);
    assert!(rectangles.is_empty(), "{rectangles:?}");
}

#[test]
fn a_day_before_the_range_takes_the_plain_descriptions() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(day_in_utc_millis(2021, 3, 6))
        )
    });
    // No end yet: Compose's day tells nothing of the range.
    ui.on_node(day("Saturday, March 6, 2021")).assert_is_selected();
    ui.on_node(day("Start date, Saturday, March 6, 2021")).assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DateRangePickerScreenshotTest.kt, dateRangePicker_withModeToggle
#[test]
fn the_titles_follow_the_mode() {
    let mut ui = range_picker(|| remember_date_range_picker_state!());
    ui.on_node_with_text("Select dates").assert_exists();
    ui.on_node_with_content_description("Switch to text input mode").perform_click();
    ui.wait_for_idle();
    ui.on_node_with_text("Enter dates").assert_exists();
    ui.on_node_with_text("Select dates").assert_does_not_exist();
}

#[test]
fn changing_the_mode_shows_the_starts_month() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_selected_start_date_millis = Some(day_in_utc_millis(2021, 3, 6)),
            initial_displayed_month_millis = Some(day_in_utc_millis(2021, 7, 1))
        )
    });
    ui.on_node_with_text("July 2021").assert_exists();
    state().set_display_mode(DisplayMode::Input);
    assert_eq!(state().displayed_month_millis(), day_in_utc_millis(2021, 3, 1));
    ui.wait_for_idle();
}

#[test]
fn the_months_tell_of_scrolling_as_custom_actions() {
    let mut ui = range_picker(|| {
        remember_date_range_picker_state!(
            initial_displayed_month_millis = Some(day_in_utc_millis(2021, 3, 1))
        )
    });
    ui.on_node_with_text("March 2021")
        .perform_custom_accessibility_action_with_label("Scroll to show the next month");
    ui.wait_for_idle();
    assert_eq!(state().displayed_month_millis(), day_in_utc_millis(2021, 4, 1));
    ui.on_node_with_text("April 2021")
        .perform_custom_accessibility_action_with_label("Scroll to show the previous month");
    ui.wait_for_idle();
    assert_eq!(state().displayed_month_millis(), day_in_utc_millis(2021, 3, 1));
}
