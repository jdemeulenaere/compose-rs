//! Compose's `TimePickerTest`, at density 1 in a 400 by 800 window. A dial's test goes straight to
//! the hidden `ClockFace`, as Compose's goes to the internal one. Compose's `AnalogTimePickerState`
//! is `TimePickerState` itself here (D1566), its angle hidden. Not ported: the vibrant variants and
//! `TimeScroll` (not there yet), and the large fonts' overflow (`Text`'s `autoSize` isn't there).

use std::cell::{Cell, RefCell};
use std::f32::consts::PI;
use std::rc::Rc;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{
    Color, Configuration, Density, InputMode, IntSize, Key, LOCAL_CONFIGURATION,
    LOCAL_INPUT_MODE_MANAGER, LOCAL_LAYOUT_DIRECTION, LOCAL_LOCALE_LIST, LayoutDirection,
    LiveRegionMode, LocaleList, Modifier, Offset, Role, SemanticsActions, SemanticsProperties,
};

use crate::date_picker::RecordingColors;

fn twelve_hours(hour: i32, minute: i32) -> Owned<TimePickerState> {
    TimePickerState::new(hour, minute, false, TimePickerSelectionMode::Hour)
}

fn twenty_four_hours(hour: i32, minute: i32) -> Owned<TimePickerState> {
    TimePickerState::new(hour, minute, true, TimePickerSelectionMode::Hour)
}

fn time_picker(state: TimePickerState) -> ComposeTestRule {
    compose_test(move || {
        MaterialTheme!({
            TimePicker!(state);
        });
    })
}

fn time_input(state: TimePickerState) -> ComposeTestRule {
    compose_test(move || {
        MaterialTheme!({
            TimeInput!(state);
        });
    })
}

fn clock_face(state: TimePickerState, auto_switch_to_minute: bool) -> ComposeTestRule {
    compose_test(move || {
        MaterialTheme!({
            ClockFace!(Modifier, state, TimePickerDefaults::colors(), auto_switch_to_minute);
        });
    })
}

fn description_of(number: i32, selection: TimePickerSelectionMode, is_24_hour: bool) -> String {
    match selection {
        TimePickerSelectionMode::Minute => format!("{number} minutes"),
        TimePickerSelectionMode::Hour if is_24_hour => format!("{number} hours"),
        TimePickerSelectionMode::Hour => format!("{number} o'clock"),
    }
}

fn time_value(
    ui: &mut ComposeTestRule,
    number: i32,
    selection: TimePickerSelectionMode,
    is_24_hour: bool,
) -> SemanticsNodeInteraction<'_> {
    ui.on_all_nodes_with_content_description(&description_of(number, selection, is_24_hour))
        .on_first()
}

fn has_role(role: Role) -> Matcher {
    Matcher::new(format!("with the role {role:?}"), move |config| {
        config.get(&SemanticsProperties::ROLE).is_some_and(|value| *value == role)
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_vertical_layout
#[test]
fn a_tall_screen_lays_out_vertically() {
    let seen = Rc::new(Cell::new(None));
    let record = seen.clone();
    compose_test(move || {
        let record = record.clone();
        let tall = Configuration {
            screen_height_dp: 800.0,
            screen_width_dp: 500.0,
            ..LOCAL_CONFIGURATION.current()
        };
        CompositionLocalProvider(LOCAL_CONFIGURATION.provides(tall), move || {
            record.set(Some(TimePickerDefaults::layout_type()));
        });
    });
    assert_eq!(seen.get(), Some(TimePickerLayoutType::Vertical));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_horizontal_layout
#[test]
fn a_wide_screen_lays_out_horizontally() {
    let seen = Rc::new(Cell::new(None));
    let record = seen.clone();
    compose_test(move || {
        let record = record.clone();
        let wide = Configuration {
            screen_height_dp: 500.0,
            screen_width_dp: 800.0,
            ..LOCAL_CONFIGURATION.current()
        };
        CompositionLocalProvider(LOCAL_CONFIGURATION.provides(wide), move || {
            record.set(Some(TimePickerDefaults::layout_type()));
        });
    });
    assert_eq!(seen.get(), Some(TimePickerLayoutType::Horizontal));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_initialState
#[test]
fn the_initial_time_is_shown_and_its_period_selected() {
    let state = twelve_hours(14, 23);
    let mut ui = time_picker(*state);
    time_value(&mut ui, 2, TimePickerSelectionMode::Hour, false).assert_is_selected();
    time_value(&mut ui, 23, TimePickerSelectionMode::Minute, false).assert_exists();
    ui.on_node_with_text("AM").assert_exists();
    ui.on_node_with_text("PM").assert_exists().assert_is_selected();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_switchToMinutes
#[test]
fn clicking_the_minutes_goes_to_them() {
    let state = twelve_hours(14, 23);
    let mut ui = time_picker(*state);
    time_value(&mut ui, 23, TimePickerSelectionMode::Minute, false).perform_click();
    ui.wait_for_idle();
    assert_eq!(state.selection(), TimePickerSelectionMode::Minute);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_selectHour
#[test]
fn clicking_an_hour_on_the_dial_keeps_the_period() {
    let state = twelve_hours(14, 23);
    let mut ui = time_picker(*state);
    time_value(&mut ui, 6, TimePickerSelectionMode::Hour, false).perform_click();
    ui.wait_for_idle();
    assert_eq!(state.hour(), 18);
    ui.on_node_with_text("06").assert_exists();
    if state.selection() == TimePickerSelectionMode::Hour {
        ui.on_node_with_text("23").perform_click();
        ui.wait_for_idle();
    }
    ui.on_node_with_text("23").assert_is_selected();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_selectHour_a11y
#[test]
fn a_keyboards_click_stays_on_the_hours() {
    let state = twelve_hours(14, 23);
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            LOCAL_INPUT_MODE_MANAGER.current().request_input_mode(InputMode::Keyboard);
            TimePicker!(handle);
        });
    });
    time_value(&mut ui, 9, TimePickerSelectionMode::Hour, false)
        .assert_has_click_action()
        .perform_semantics_action(&SemanticsActions::ON_CLICK);
    ui.wait_for_idle();
    assert_eq!(state.hour(), 21);
    ui.on_node_with_text("09").assert_is_selected();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_invalidHour_showsErrorAndTriggersLiveRegion
#[test]
fn an_invalid_hour_says_so_politely() {
    let state = twenty_four_hours(10, 30);
    let mut ui = time_input(*state);
    replace_text(&mut ui, has_content_description("for hour"), "25");
    ui.wait_for_idle();
    ui.on_node_with_text("Hour must be 0–23").assert_exists().assert(Matcher::new(
        "a polite live region",
        |config| {
            config
                .get(&SemanticsProperties::LIVE_REGION)
                .is_some_and(|mode| *mode == LiveRegionMode::Polite)
        },
    ));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_switchToAM
#[test]
fn am_takes_twelve_hours_off() {
    let state = twelve_hours(14, 23);
    let mut ui = time_picker(*state);
    assert_eq!(state.hour(), 14);
    ui.on_node_with_text("AM").perform_click();
    ui.wait_for_idle();
    assert_eq!(state.hour(), 2);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_dragging
#[test]
fn dragging_from_the_top_to_the_right_is_three() {
    let state = twelve_hours(0, 23);
    let mut ui = time_picker(*state);
    ui.on_all_nodes_unmerged(is_selectable_group()).on_last().perform_touch_input(|touch| {
        let (top, right) = (touch.top_center(), touch.center_right());
        touch.down(top);
        touch.move_to(right);
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(state.hour(), 3);
}

fn is_selectable_group() -> Matcher {
    Matcher::new("a selectable group", |config| {
        config.get(&SemanticsProperties::SELECTABLE_GROUP).is_some()
    })
}

fn format_under(tag: &'static str) -> bool {
    let seen = Rc::new(Cell::new(None));
    let record = seen.clone();
    let _ui = compose_test(move || {
        let record = record.clone();
        CompositionLocalProvider(
            LOCAL_LOCALE_LIST.provides(LocaleList::from_language_tags(tag)),
            move || {
                record.set(Some(remember_time_picker_state!().is_24_hour()));
            },
        );
    });
    seen.get().unwrap()
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePickerState_format_12h_fromConfig
#[test]
fn the_us_uses_twelve_hours() {
    assert!(!format_under("en-US"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePickerState_format_24h_fromConfig
#[test]
fn france_uses_twenty_four_hours() {
    assert!(format_under("fr-FR"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_toggle_semantics
#[test]
fn the_period_toggles_children_are_selectable_buttons() {
    let state = twelve_hours(14, 23);
    let mut ui = time_picker(*state);
    let mut buttons = ui
        .on_node_with_content_description("Select AM or PM")
        .on_children()
        .filter(has_role(Role::Button));
    buttons.assert_count_equals(2);
    buttons.assert_all(is_selectable());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_display_semantics
#[test]
fn the_display_is_a_pair_of_radio_buttons() {
    let state = twelve_hours(14, 23);
    let mut ui = time_picker(*state);
    ui.on_node_with_content_description("Select minutes")
        .assert_is_selectable()
        .assert_is_not_selected()
        .assert(has_role(Role::RadioButton))
        .assert_has_click_action();
    ui.on_node_with_content_description("Select hour")
        .assert_is_selectable()
        .assert_is_selected()
        .assert(has_role(Role::RadioButton))
        .assert_has_click_action();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_clockFace_hour_semantics
#[test]
fn the_dials_hours_are_read_out_as_oclock() {
    let state = twelve_hours(14, 23);
    let mut ui = time_picker(*state);
    let mut siblings =
        ui.on_all_nodes_with_content_description("2 o'clock").on_last().on_siblings();
    siblings.assert_count_equals(11);
    siblings.assert_all(has_content_description_with("o'clock", true, true));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_clockFace_selected_semantics
#[test]
fn the_selected_hour_of_the_dial_is_selected() {
    let state = twenty_four_hours(14, 23);
    let mut ui = time_picker(*state);
    ui.wait_for_idle();
    ui.on_all_nodes(has_text("14")).filter(is_focusable()).assert_all(is_selected());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_clockFace_minutes_semantics
#[test]
fn the_dials_minutes_are_read_out_as_minutes() {
    let state = twelve_hours(14, 23);
    let mut ui = time_picker(*state);
    ui.on_node_with_text("23").perform_click();
    ui.wait_for_idle();
    let mut siblings = ui.on_node_with_content_description("55 minutes").on_siblings();
    siblings.assert_count_equals(11);
    siblings.assert_all(has_content_description_with("minutes", true, true));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_semantics
#[test]
fn the_time_inputs_fields_tell_what_they_are() {
    let state = twenty_four_hours(14, 23);
    let mut ui = time_input(*state);
    ui.on_node_with_text("14")
        .assert(is_focusable())
        .assert_content_description_contains("for hour", true, false)
        .assert(has_ime_action(compose_foundation::ImeAction::Next))
        .assert(is_focused());
    ui.on_all_nodes(has_text("23"))
        .filter_to_one(is_selectable())
        .assert(is_not_selected())
        .perform_click();
    ui.wait_for_idle();
    ui.on_node_with_text("23").assert_content_description_contains("for minutes", true, false);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_userOverride_updates
#[test]
fn the_fields_follow_the_state() {
    let state = twenty_four_hours(14, 0);
    let mut ui = time_input(*state);
    ui.on_node_with_text("14")
        .assert(is_focusable())
        .assert_content_description_contains("for hour", true, false);
    state.set_hour(20);
    state.set_minute(12);
    ui.wait_for_idle();
    ui.on_node_with_text("20").assert_content_description_contains("for hour", true, false);
    ui.on_all_nodes(has_text("12")).assert_count_equals(2);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_revertedInput_userOverride_stillUpdates
#[test]
fn a_rejected_key_doesnt_stop_the_fields_following() {
    let state = twenty_four_hours(14, 0);
    let mut ui = time_input(*state);
    press(&mut ui, "14", &[Key::Character('a')]);
    state.set_hour(20);
    state.set_minute(12);
    ui.wait_for_idle();
    ui.on_node_with_text("20").assert_content_description_contains("for hour", true, false);
    ui.on_all_nodes(has_text("12")).assert_count_equals(2);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_userOverride_updates
#[test]
fn the_display_follows_the_state() {
    let state = twenty_four_hours(14, 0);
    let mut ui = time_picker(*state);
    ui.on_node_with_text("14").assert_is_selected();
    state.set_hour(20);
    ui.wait_for_idle();
    ui.on_node_with_text("20").assert_is_selected();
}

/// As `performTextReplacement`: the field's text action.
fn replace_text(ui: &mut ComposeTestRule, field: Matcher, text: &str) {
    ui.on_node(field).perform_semantics_action_with(&SemanticsActions::SET_TEXT, |set| {
        set(text.into());
    });
}

fn press(ui: &mut ComposeTestRule, text: &str, keys_pressed: &[Key]) {
    ui.on_node_with_text(text).assert_exists();
    press_on_focused(ui, keys_pressed);
}

/// A digit arrives at a field as committed text here, as from an input method; the other keys as
/// keys.
fn press_on_focused(ui: &mut ComposeTestRule, keys_pressed: &[Key]) {
    for key in keys_pressed {
        match key {
            Key::Character(character) => ui.type_text(&character.to_string()),
            _ => {
                ui.on_node(is_focused()).perform_key_input(|keys| keys.press_key(*key));
            }
        }
        ui.wait_for_idle();
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_keyboardInput_valid
#[test]
fn typing_both_fields_sets_the_time() {
    let state = twelve_hours(10, 23);
    let mut ui = time_input(*state);
    press(&mut ui, "10", &[Key::Character('0'), Key::Character('4')]);
    press(&mut ui, "23", &[Key::Character('5'), Key::Character('2')]);
    assert_eq!(state.minute(), 52);
    assert_eq!(state.hour(), 4);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_keyboardInput_outOfRange
#[test]
fn only_the_first_digit_of_an_out_of_range_hour_counts() {
    let state = twelve_hours(10, 23);
    let mut ui = time_input(*state);
    press(&mut ui, "10", &[Key::Character('4'), Key::Character('4')]);
    assert_eq!(state.hour(), 4);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_keyboardInput_Nan
#[test]
fn what_isnt_a_digit_changes_nothing() {
    let state = twelve_hours(10, 23);
    let mut ui = time_input(*state);
    press(
        &mut ui,
        "10",
        &[
            Key::Character('a'),
            Key::Character('b'),
            Key::Character('c'),
            Key::Character('.'),
            Key::Character(','),
            Key::Character(','),
        ],
    );
    assert_eq!(state.hour(), 10);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_keyboardInput_switchAmPm
#[test]
fn pm_adds_twelve_hours() {
    let state = twelve_hours(10, 23);
    let mut ui = time_input(*state);
    ui.on_node_with_text("PM").perform_click();
    ui.wait_for_idle();
    assert_eq!(state.hour(), 22);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_keyboardInput_maintainsPm
#[test]
fn typing_an_hour_keeps_pm() {
    let state = twelve_hours(23, 23);
    let mut ui = time_input(*state);
    assert!(state.is_pm());
    press(&mut ui, "11", &[Key::Character('4')]);
    assert!(state.is_pm());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_input12_maintainsAm
#[test]
fn twelve_in_the_morning_is_midnight() {
    let state = twelve_hours(10, 0);
    let mut ui = time_input(*state);
    press(&mut ui, "10", &[Key::Character('1'), Key::Character('2')]);
    assert!(!state.is_pm());
    assert_eq!(state.hour(), 0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_input12_resultsIn12
#[test]
fn twelve_in_the_afternoon_is_noon() {
    let state = twelve_hours(14, 0);
    let mut ui = time_input(*state);
    press(&mut ui, "02", &[Key::Character('1'), Key::Character('2')]);
    assert!(state.is_pm());
    assert_eq!(state.hour(), 12);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_deleting_maintainsPm
#[test]
fn deleting_an_hour_keeps_pm() {
    let state = twelve_hours(23, 23);
    let mut ui = time_input(*state);
    assert!(state.is_pm());
    press(&mut ui, "11", &[Key::Delete, Key::Delete]);
    assert!(state.is_pm());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_24Hour_noAmPm_Toggle
#[test]
fn twenty_four_hours_have_no_periods() {
    let state = twenty_four_hours(22, 23);
    let mut ui = time_input(*state);
    ui.on_node_with_text("PM").assert_does_not_exist();
    ui.on_node_with_text("AM").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_24Hour_writePmHour
#[test]
fn twenty_four_hours_take_an_afternoon_hour() {
    let state = twenty_four_hours(10, 23);
    let mut ui = time_input(*state);
    press(&mut ui, "10", &[Key::Character('2'), Key::Character('2')]);
    assert_eq!(state.hour(), 22);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_24HourStartingPm_writePmHour
#[test]
fn twenty_four_hours_starting_in_the_afternoon_take_one_too() {
    let state = twenty_four_hours(20, 23);
    let mut ui = time_input(*state);
    press(&mut ui, "20", &[Key::Character('2'), Key::Character('2')]);
    assert_eq!(state.hour(), 22);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_24Hour_writeNoon
#[test]
fn twenty_four_hours_take_noon() {
    let state = twenty_four_hours(10, 23);
    let mut ui = time_input(*state);
    press(&mut ui, "10", &[Key::Character('1'), Key::Character('2')]);
    assert_eq!(state.hour(), 12);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_writeMinute_updatesCurrentAngle
#[test]
fn a_typed_minute_moves_the_dials_angle() {
    let state = twenty_four_hours(10, 0);
    let mut ui = time_input(*state);
    press(&mut ui, "10", &[Key::Character('1'), Key::Character('2')]);
    press(&mut ui, "00", &[Key::Character('4'), Key::Character('5')]);
    assert!((state.current_angle() - PI).abs() < 0.0001, "{}", state.current_angle());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_24Hour_writeMidnight
#[test]
fn twenty_four_hours_take_midnight() {
    let state = twenty_four_hours(10, 23);
    let mut ui = time_input(*state);
    press(&mut ui, "10", &[Key::Character('0'), Key::Character('0')]);
    assert_eq!(state.hour(), 0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, state_restoresTimePickerState
#[test]
fn a_time_state_is_saved_and_restored() {
    thread_local! {
        static STATE: Cell<Option<TimePickerState>> = const { Cell::new(None) };
    }
    let state = || STATE.with(Cell::get).unwrap();
    let mut rule = compose_test(|| {});
    let mut tester = StateRestorationTester::new();
    tester.set_content(&mut rule, || {
        let state =
            remember_time_picker_state!(initial_hour = 14, initial_minute = 54, is_24_hour = true);
        STATE.with(|cell| cell.set(Some(state)));
    });
    rule.run_on_idle(|_| state().set_selection(TimePickerSelectionMode::Minute));
    STATE.with(|cell| cell.set(None));
    tester.emulate_save_and_restore(&mut rule);
    assert_eq!(state().hour(), 14);
    assert_eq!(state().minute(), 54);
    assert!(state().is_24_hour());
    assert_eq!(state().selection(), TimePickerSelectionMode::Minute);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, state_setHour_updatesIsPm
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, analogState_setHour_updatesIsPm
#[test]
fn setting_an_hour_updates_the_period() {
    let state = twelve_hours(8, 0);
    state.set_hour(20);
    assert!(state.is_pm());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, state_12h_hourInitializationMatches
#[test]
fn twelve_hours_keep_their_initial_hour() {
    for hour in 0..24 {
        assert_eq!(twelve_hours(hour, 0).hour(), hour);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, state_24h_hourInitializationMatches
#[test]
fn twenty_four_hours_keep_their_initial_hour() {
    for hour in 0..24 {
        assert_eq!(twenty_four_hours(hour, 0).hour(), hour);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, clockFace_24Hour_everyValue
#[test]
fn every_hour_of_twenty_four_can_be_clicked() {
    let state = twenty_four_hours(10, 23);
    let mut ui = clock_face(*state, true);
    for number in 0..24 {
        time_value(&mut ui, number, TimePickerSelectionMode::Hour, true).perform_click();
        ui.wait_for_idle();
        state.set_selection(TimePickerSelectionMode::Hour);
        ui.wait_for_idle();
        assert_eq!(state.hour(), number);
        time_value(&mut ui, number, TimePickerSelectionMode::Hour, true).assert_is_selected();
    }
}

fn every_value_by_keyboard(state: TimePickerState, count: i32, is_24_hour: bool) {
    state.set_is_dial_focusable(true);
    let mut ui = compose_test(move || {
        MaterialTheme!({
            LOCAL_INPUT_MODE_MANAGER.current().request_input_mode(InputMode::Keyboard);
            ClockFace!(Modifier, state, TimePickerDefaults::colors(), false);
        });
    });
    ui.wait_for_idle();
    ui.on_root().perform_key_input(|keys| keys.press_key(Key::Tab));
    ui.wait_for_idle();
    for number in 0..count {
        let shown = if !is_24_hour && number == 0 { 12 } else { number };
        let hour = TimePickerSelectionMode::Hour;
        time_value(&mut ui, shown, hour, is_24_hour).assert_is_focused();
        time_value(&mut ui, shown, hour, is_24_hour)
            .perform_key_input(|keys| keys.press_key(Key::Enter));
        ui.wait_for_idle();
        state.set_selection(hour);
        state.set_is_dial_focusable(true);
        assert_eq!(state.hour(), number);
        ui.wait_for_idle();
        time_value(&mut ui, shown, hour, is_24_hour).assert_is_selected();
        time_value(&mut ui, shown, hour, is_24_hour).request_focus();
        ui.wait_for_idle();
        time_value(&mut ui, shown, hour, is_24_hour)
            .perform_key_input(|keys| keys.press_key(Key::DirectionRight));
        ui.wait_for_idle();
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, clockFace_24Hour_everyValue_byKeyboard
#[test]
fn every_hour_of_twenty_four_can_be_reached_by_keys() {
    let state = twenty_four_hours(0, 23);
    every_value_by_keyboard(*state, 24, true);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, clockFace_12Hour_everyValue
#[test]
fn every_hour_of_twelve_can_be_clicked() {
    let state = twelve_hours(0, 0);
    let mut ui = clock_face(*state, true);
    for number in 0..12 {
        let shown = if number == 0 { 12 } else { number };
        time_value(&mut ui, shown, TimePickerSelectionMode::Hour, false).perform_click();
        ui.wait_for_idle();
        state.set_selection(TimePickerSelectionMode::Hour);
        ui.wait_for_idle();
        assert_eq!(state.hour(), number);
        time_value(&mut ui, shown, TimePickerSelectionMode::Hour, false).assert_is_selected();
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, clockFace_12Hour_everyValue_byKeyboard
#[test]
fn every_hour_of_twelve_can_be_reached_by_keys() {
    let state = twelve_hours(0, 0);
    every_value_by_keyboard(*state, 12, false);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, clockFace_12Hour_traversalIndex
#[test]
fn the_dial_is_read_in_order_from_the_top() {
    let state = twelve_hours(0, 0);
    let mut ui = clock_face(*state, true);
    for number in 0..12 {
        let shown = if number == 0 { 12 } else { number };
        time_value(&mut ui, shown, TimePickerSelectionMode::Hour, false)
            .assert(Matcher::new("the traversal index", move |config| {
                config
                    .get(&SemanticsProperties::TRAVERSAL_INDEX)
                    .is_some_and(|index| *index == number as f32 + 1.0)
            }))
            .perform_click();
        ui.wait_for_idle();
        state.set_selection(TimePickerSelectionMode::Hour);
        ui.wait_for_idle();
        assert_eq!(state.hour(), number);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, clockFace_12Hour_initAtNoon
#[test]
fn noon_is_the_afternoon() {
    let state = twelve_hours(12, 0);
    assert!(state.is_pm());
    assert_eq!(state.hour(), 12);
}

fn every_minute(is_24_hour: bool) {
    let state = TimePickerState::new(10, 23, is_24_hour, TimePickerSelectionMode::Minute);
    let mut ui = clock_face(*state, true);
    for number in 0..11 {
        time_value(&mut ui, number * 5, TimePickerSelectionMode::Minute, is_24_hour)
            .perform_click();
        ui.wait_for_idle();
        assert_eq!(state.minute(), number * 5);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, clockFace_24HourMinutes_everyValue
#[test]
fn every_five_minutes_can_be_clicked_with_twenty_four_hours() {
    every_minute(true);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, clockFace_12HourMinutes_everyValue
#[test]
fn every_five_minutes_can_be_clicked_with_twelve_hours() {
    every_minute(false);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_showsError_forInvalidHour
#[test]
fn an_invalid_hour_can_be_typed_away() {
    let state = twenty_four_hours(10, 0);
    let mut ui = time_input(*state);
    press(&mut ui, "10", &[Key::Character('2'), Key::Character('7')]);
    assert!(!state.is_hour_input_valid());
    state.set_selection(TimePickerSelectionMode::Hour);
    ui.wait_for_idle();
    press_on_focused(
        &mut ui,
        &[Key::Backspace, Key::Backspace, Key::Character('1'), Key::Character('2')],
    );
    assert!(state.is_hour_input_valid());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_showsError_forInvalidMinute
#[test]
fn an_invalid_minute_can_be_typed_away() {
    let state = TimePickerState::new(10, 0, true, TimePickerSelectionMode::Minute);
    let mut ui = time_input(*state);
    press(&mut ui, "00", &[Key::Character('9'), Key::Character('9')]);
    assert!(!state.is_minute_input_valid());
    press(
        &mut ui,
        "99",
        &[Key::Backspace, Key::Backspace, Key::Character('1'), Key::Character('2')],
    );
    assert!(state.is_minute_input_valid());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_updateFocus_validInput
#[test]
fn a_valid_hour_goes_on_to_the_minutes() {
    let state = twenty_four_hours(10, 0);
    let mut ui = time_input(*state);
    replace_text(&mut ui, has_text("10"), "12");
    ui.wait_for_idle();
    press_on_focused(&mut ui, &[Key::Character('0')]);
    assert_eq!(state.selection(), TimePickerSelectionMode::Minute);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_maintainFocus_invalidInput
#[test]
fn an_invalid_hour_stays_on_the_hours() {
    let state = twenty_four_hours(10, 0);
    let mut ui = time_input(*state);
    replace_text(&mut ui, has_text("10"), "27");
    ui.wait_for_idle();
    assert_eq!(state.selection(), TimePickerSelectionMode::Hour);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_12Hour_validHour_autoAdvancesFocus
#[test]
fn two_digits_go_on_and_the_next_is_a_minute() {
    let state = twelve_hours(8, 0);
    let mut ui = time_input(*state);
    press(&mut ui, "08", &[Key::Character('0'), Key::Character('1'), Key::Character('3')]);
    assert!(state.is_hour_input_valid());
    assert_eq!(state.selection(), TimePickerSelectionMode::Minute);
    assert_eq!(state.minute(), 3);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_12Hour_delete_doesNotAdvanceFocus
#[test]
fn deleting_doesnt_go_on() {
    let state = twelve_hours(8, 0);
    let mut ui = time_input(*state);
    press(&mut ui, "08", &[Key::Delete]);
    assert_eq!(state.selection(), TimePickerSelectionMode::Hour);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_12Hour_singleDigit2To9_autoAdvancesFocus
#[test]
fn a_digit_no_twelve_hour_starts_with_goes_on() {
    let state = twelve_hours(10, 0);
    let mut ui = time_input(*state);
    press(&mut ui, "10", &[Key::Character('4')]);
    assert_eq!(state.hour(), 4);
    assert!(state.is_hour_input_valid());
    assert_eq!(state.selection(), TimePickerSelectionMode::Minute);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_12Hour_singleDigitOne_doesNotAutoAdvanceFocus
#[test]
fn a_one_might_be_ten_to_twelve_so_it_stays() {
    let state = twelve_hours(10, 0);
    let mut ui = time_input(*state);
    press(&mut ui, "10", &[Key::Character('1')]);
    assert_eq!(state.hour(), 1);
    assert!(state.is_hour_input_valid());
    assert_eq!(state.selection(), TimePickerSelectionMode::Hour);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_24Hour_singleDigit_doesNotAutoAdvanceFocus
#[test]
fn a_digit_with_twenty_four_hours_stays() {
    let state = twenty_four_hours(10, 0);
    let mut ui = time_input(*state);
    press(&mut ui, "10", &[Key::Character('4')]);
    assert_eq!(state.hour(), 4);
    assert!(state.is_hour_input_valid());
    assert_eq!(state.selection(), TimePickerSelectionMode::Hour);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timeInput_minute_imeDone_invokesDefaultAction
#[test]
fn done_on_the_minutes_hides_the_keyboard() {
    let state = TimePickerState::new(10, 23, false, TimePickerSelectionMode::Minute);
    let mut ui = time_input(*state);
    ui.wait_for_idle();
    ui.host().take_software_keyboard_request();
    ui.on_node_with_text("23").perform_ime_action();
    ui.wait_for_idle();
    assert_eq!(ui.host().take_software_keyboard_request(), Some(false));
}

#[test]
fn the_period_buttons_follow_each_other() {
    let state = twelve_hours(9, 0);
    let mut ui = time_input(*state);
    ui.on_node_with_text("AM").assert_is_selected();
    ui.on_node_with_text("PM").assert_is_not_selected();
    ui.on_node_with_text("PM").perform_click();
    ui.wait_for_idle();
    ui.on_node_with_text("PM").assert_is_selected();
    ui.on_node_with_text("AM").assert_is_not_selected();
}

#[test]
fn the_dial_follows_the_state() {
    let state = twelve_hours(0, 0);
    let mut ui = time_picker(*state);
    let at_midnight = state.current_angle();
    state.set_hour(3);
    ui.wait_for_idle();
    assert!((state.current_angle() - (at_midnight + PI / 2.0)).abs() < 0.0001);
}

#[test]
fn an_hour_out_of_range_can_not_start_a_state() {
    let result = std::panic::catch_unwind(|| twelve_hours(24, 0));
    assert!(result.is_err());
    let result = std::panic::catch_unwind(|| twelve_hours(0, 60));
    assert!(result.is_err());
}

#[test]
fn inputs_out_of_range_dont_reach_the_time() {
    let state = twelve_hours(14, 30);
    // Twelve hours in the afternoon: only 12 to 23.
    state.set_hour_input(3);
    assert_eq!(state.hour(), 14);
    assert_eq!(state.hour_input(), 3);
    assert!(!state.is_hour_input_valid());
    assert!(!state.is_input_valid());
    state.set_hour_input(15);
    assert_eq!(state.hour(), 15);
    assert!(state.is_hour_input_valid());
    state.set_minute_input(60);
    assert_eq!(state.minute(), 30);
    assert_eq!(state.minute_input(), 60);
    assert!(!state.is_minute_input_valid());
    state.set_minute_input(59);
    assert_eq!(state.minute(), 59);
    assert!(state.is_input_valid());
}

#[test]
fn every_hour_can_be_clicked_on_a_smaller_dial() {
    let state = twenty_four_hours(10, 23);
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            ClockFace!(
                Modifier.size(compose_ui::Dp(200.0)),
                handle,
                TimePickerDefaults::colors(),
                true
            );
        });
    });
    for number in 0..24 {
        time_value(&mut ui, number, TimePickerSelectionMode::Hour, true).perform_click();
        ui.wait_for_idle();
        state.set_selection(TimePickerSelectionMode::Hour);
        ui.wait_for_idle();
        assert_eq!(state.hour(), number);
    }
}

/// The dial's width in a window `height` high, laid out beside the time.
fn dial_width_in(height: i32) -> f32 {
    let state = twenty_four_hours(10, 23);
    let handle = *state;
    let mut ui = compose_test_with(
        IntSize::new(800, height),
        Density::default(),
        Box::new(compose_ui::platform::FixedTextMeasurer::new()),
        move || {
            MaterialTheme!({
                TimePicker!(handle, layout_type = TimePickerLayoutType::Horizontal);
            });
        },
    );
    ui.wait_for_idle();
    let bounds = ui.on_all_nodes_unmerged(is_selectable_group()).on_last().get_bounds_in_root();
    bounds.right.0 - bounds.left.0
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerSizeTest.kt, clockFace_24Hour_everyValue
#[test]
fn every_hour_can_be_clicked_in_each_window() {
    for size in [384, 350, 300] {
        let state = twenty_four_hours(10, 23);
        let handle = *state;
        let mut ui = compose_test_with(
            IntSize::new(size, size),
            Density::default(),
            Box::new(compose_ui::platform::FixedTextMeasurer::new()),
            move || {
                MaterialTheme!({
                    ClockFace!(Modifier, handle, TimePickerDefaults::colors(), true);
                });
            },
        );
        for number in 0..24 {
            time_value(&mut ui, number, TimePickerSelectionMode::Hour, true).perform_click();
            ui.wait_for_idle();
            state.set_selection(TimePickerSelectionMode::Hour);
            ui.wait_for_idle();
            assert_eq!(state.hour(), number);
            time_value(&mut ui, number, TimePickerSelectionMode::Hour, true).assert_is_selected();
        }
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerSizeTest.kt, clockFace_size_resizesCorrectly
#[test]
fn the_dial_is_as_large_as_the_height_allows() {
    assert_eq!(dial_width_in(384), 256.0);
    assert_eq!(dial_width_in(350), 238.0);
    assert_eq!(dial_width_in(300), 200.0);
}

type ColorsLog = Rc<RefCell<Vec<(String, Color)>>>;

const SELECTOR: Color = Color::from_rgb(1, 2, 3);
const NUMBER: Color = Color::from_rgb(4, 5, 6);
const NUMBER_SELECTED: Color = Color::from_rgb(7, 8, 9);

/// The ovals in the selector's color, as center and radius, and the texts' colors.
fn selector_drawn(state: TimePickerState) -> (Vec<(f32, f32, f32)>, ColorsLog) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut ui = compose_test_with(
        IntSize::new(400, 800),
        Density::default(),
        Box::new(RecordingColors {
            inner: compose_ui::platform::FixedTextMeasurer::new(),
            log: log.clone(),
        }),
        move || {
            MaterialTheme!({
                ClockFace!(
                    Modifier.size(compose_ui::Dp(256.0)),
                    state,
                    TimePickerColors {
                        selector_color: SELECTOR,
                        clock_dial_content_color: NUMBER,
                        clock_dial_selected_content_color: NUMBER_SELECTED,
                        ..TimePickerDefaults::colors()
                    },
                    false
                );
            });
        },
    );
    ui.wait_for_idle();
    let mut stack = vec![Offset::ZERO];
    let mut ovals = Vec::new();
    for op in &ui.host().display_list().ops {
        let at = *stack.last().expect("a translation");
        match op {
            DrawOp::Save | DrawOp::SaveLayerAlpha { .. } => stack.push(at),
            DrawOp::Restore => {
                stack.pop();
            }
            DrawOp::Translate(by) => *stack.last_mut().expect("a translation") = at + *by,
            DrawOp::DrawArc { oval, paint, .. } if paint.color == SELECTOR => ovals.push((
                ((oval.left + oval.right) / 2.0 + at.x).round(),
                ((oval.top + oval.bottom) / 2.0 + at.y).round(),
                ((oval.right - oval.left) / 2.0).round(),
            )),
            _ => {}
        }
    }
    (ovals, log)
}

fn last_color(log: &ColorsLog, text: &str) -> Option<Color> {
    log.borrow().iter().rev().find(|(logged, _)| logged == text).map(|(_, color)| *color)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerScreenshotTest.kt, timePicker_12h
#[test]
fn the_selector_is_at_three_oclock_and_its_number_in_the_selected_color() {
    let state = twelve_hours(3, 0);
    let (ovals, log) = selector_drawn(*state);
    // The handle 101 from the dial's center, on the right; the small dot at the center.
    assert_eq!(ovals, vec![(229.0, 128.0, 24.0), (128.0, 128.0, 4.0)]);
    assert_eq!(last_color(&log, "3"), Some(NUMBER_SELECTED));
    assert_eq!(last_color(&log, "4"), Some(NUMBER));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerScreenshotTest.kt, timePicker_24h
#[test]
fn an_afternoon_hour_is_on_the_inner_circle() {
    let state = twenty_four_hours(15, 0);
    let (ovals, log) = selector_drawn(*state);
    assert_eq!(ovals, vec![(197.0, 128.0, 24.0), (128.0, 128.0, 4.0)]);
    assert_eq!(last_color(&log, "15"), Some(NUMBER_SELECTED));
    assert_eq!(last_color(&log, "3"), Some(NUMBER));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerScreenshotTest.kt, timePicker_12h_rtl
#[test]
fn a_right_to_left_dial_is_a_clock_all_the_same() {
    let state = twelve_hours(3, 0);
    let handle = *state;
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), || {
            MaterialTheme!({
                ClockFace!(
                    Modifier.size(compose_ui::Dp(256.0)),
                    handle,
                    TimePickerDefaults::colors(),
                    false
                );
            });
        });
    });
    ui.wait_for_idle();
    let center = |bounds: compose_ui::DpRect| (bounds.left.0 + bounds.right.0) / 2.0;
    let dial =
        center(ui.on_all_nodes_unmerged(is_selectable_group()).on_last().get_bounds_in_root());
    let three = center(
        ui.on_all_nodes_with_content_description("3 o'clock").on_first().get_bounds_in_root(),
    );
    let nine = center(
        ui.on_all_nodes_with_content_description("9 o'clock").on_first().get_bounds_in_root(),
    );
    assert!(three > dial && nine < dial, "{three} {dial} {nine}");
}

#[test]
fn going_to_the_minutes_turns_the_selector_to_them() {
    let state = twelve_hours(0, 15);
    let mut ui = time_picker(*state);
    assert!((state.current_angle() + PI / 2.0).abs() < 0.0001);
    state.set_selection(TimePickerSelectionMode::Minute);
    ui.wait_for_idle();
    assert!(state.current_angle().abs() < 0.0001, "{}", state.current_angle());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimeInputScreenshotTest.kt, timeInput_12h_hourFocused
#[test]
fn the_time_inputs_places_are_the_tokens() {
    let state = twelve_hours(9, 41);
    let mut ui = time_input(*state);
    ui.wait_for_idle();
    ui.on_node_with_content_description("for hour")
        .assert_width_is_equal_to(compose_ui::Dp(96.0))
        .assert_height_is_equal_to(compose_ui::Dp(72.0));
    ui.on_node_with_content_description("Select minutes")
        .assert_width_is_equal_to(compose_ui::Dp(96.0))
        .assert_height_is_equal_to(compose_ui::Dp(72.0));
    ui.on_node_with_content_description("Select AM or PM")
        .assert_width_is_equal_to(compose_ui::Dp(52.0))
        .assert_height_is_equal_to(compose_ui::Dp(72.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerScreenshotTest.kt, timePicker_12h
#[test]
fn the_time_pickers_places_are_the_tokens() {
    let state = twelve_hours(9, 41);
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            TimePicker!(handle, layout_type = TimePickerLayoutType::Vertical);
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_content_description("Select hour")
        .assert_width_is_equal_to(compose_ui::Dp(96.0))
        .assert_height_is_equal_to(compose_ui::Dp(80.0));
    ui.on_node_with_content_description("Select AM or PM")
        .assert_width_is_equal_to(compose_ui::Dp(52.0))
        .assert_height_is_equal_to(compose_ui::Dp(80.0));
    ui.on_all_nodes_unmerged(is_selectable_group())
        .on_last()
        .assert_width_is_equal_to(compose_ui::Dp(256.0))
        .assert_height_is_equal_to(compose_ui::Dp(256.0));
    let am = ui.on_node_with_text("AM").get_bounds_in_root();
    let pm = ui.on_node_with_text("PM").get_bounds_in_root();
    assert_eq!(pm.top.0 - am.bottom.0, 4.0, "a gap of 4 between the periods");
}

fn format_with(tag: &'static str, setting: Option<bool>) -> bool {
    let seen = Rc::new(Cell::new(None));
    let record = seen.clone();
    let _ui = compose_test(move || {
        let record = record.clone();
        let configuration = Configuration { is_24_hour: setting, ..LOCAL_CONFIGURATION.current() };
        CompositionLocalProvider(
            (
                LOCAL_LOCALE_LIST.provides(LocaleList::from_language_tags(tag)),
                LOCAL_CONFIGURATION.provides(configuration),
            ),
            move || {
                record.set(Some(remember_time_picker_state!().is_24_hour()));
            },
        );
    });
    seen.get().unwrap()
}

#[test]
fn the_phones_setting_wins_over_the_locale() {
    assert!(format_with("en-US", Some(true)));
    assert!(!format_with("fr-FR", Some(false)));
}

#[test]
fn without_a_setting_the_regions_of_twelve_hours_are_the_jdks() {
    for tag in [
        "zh-TW", "zh-HK", "en-SG", "zh-MO", "km-KH", "ms-BN", "ar-DZ", "ar-TN", "ar-LY", "ar-MR",
        "ar-PS", "el-GR", "sq-AL", "el-CY", "es-PR", "es-DO", "es-PA", "en-JM", "am-ET", "en-GH",
        "en-ZM", "en-NA", "en-TT", "en-CA", "en-US", "ko-KR", "en", "ar", "ko", "hi",
    ] {
        assert!(!format_with(tag, None), "{tag}");
    }
    for tag in ["fr-CA", "fr-FR", "de-DE", "en-GB", "pt-BR", "zh-CN", "ja-JP", "es-MX", "es-ES"] {
        assert!(format_with(tag, None), "{tag}");
    }
}

#[test]
fn the_hidden_minutes_field_takes_no_focus_and_no_digits() {
    let state = twelve_hours(10, 23);
    let mut ui = time_input(*state);
    ui.wait_for_idle();
    ui.on_node_with_text("10").assert_is_focused();
    press_on_focused(&mut ui, &[Key::Tab, Key::Tab, Key::Character('7')]);
    assert_eq!(state.minute(), 23);
    assert_eq!(state.minute_input(), 23);
    assert_eq!(state.hour(), 10);
}

#[test]
fn three_digits_pasted_over_a_selection_change_nothing() {
    let state = twenty_four_hours(10, 23);
    let mut ui = time_input(*state);
    // `34` over the `1` of `10`, and `459` over all of it: neither is a digit typed into `10`.
    replace_text(&mut ui, has_text("10"), "340");
    ui.wait_for_idle();
    assert_eq!(state.hour(), 10);
    replace_text(&mut ui, has_text("10"), "459");
    ui.wait_for_idle();
    assert_eq!(state.hour(), 10);
    ui.on_node_with_text("10").assert_exists();
}

#[test]
fn a_digit_of_any_numerals_counts_and_nothing_else() {
    let state = twenty_four_hours(10, 23);
    let mut ui = time_input(*state);
    // Under en-US: Compose's `isDigit` doesn't ask the locale.
    press(&mut ui, "10", &[Key::Character('٣')]);
    assert_eq!(state.hour(), 3);
    for rejected in ['½', 'Ⅷ', '𝟏'] {
        press_on_focused(&mut ui, &[Key::Character(rejected)]);
        assert_eq!(state.hour(), 3, "{rejected}");
    }
    press_on_focused(&mut ui, &[Key::Character('٧')]);
    assert_eq!(state.hour_input(), 37);
}

fn assert_angle(state: TimePickerState, expected: f32) {
    let difference = (state.current_angle() - expected).rem_euclid(2.0 * PI);
    assert!(
        difference.min(2.0 * PI - difference) < 0.001,
        "{} against {expected}",
        state.current_angle()
    );
}

/// An outside set after a gesture that left `state` where it was.
fn nine_oclock_follows(ui: &mut ComposeTestRule, state: TimePickerState) {
    ui.wait_for_idle();
    state.set_selection(TimePickerSelectionMode::Hour);
    ui.wait_for_idle();
    state.set_hour(9);
    ui.wait_for_idle();
    assert_angle(state, PI);
}

#[test]
fn a_tap_on_the_same_hour_doesnt_lose_an_outside_set() {
    let state = twelve_hours(3, 0);
    let mut ui = time_picker(*state);
    ui.wait_for_idle();
    ui.on_all_nodes_with_content_description("3 o'clock").on_last().perform_click();
    ui.wait_for_idle();
    assert_eq!(state.hour(), 3);
    nine_oclock_follows(&mut ui, *state);
}

#[test]
fn a_drag_within_the_same_hour_doesnt_lose_an_outside_set() {
    let state = twelve_hours(3, 0);
    let mut ui = time_picker(*state);
    ui.wait_for_idle();
    ui.on_all_nodes_unmerged(is_selectable_group()).on_last().perform_touch_input(|touch| {
        touch.down(Offset::new(228.0, 128.0));
        // Inwards, past the slop, and a little round: still 3.
        touch.move_to(Offset::new(190.0, 128.0));
        touch.move_to(Offset::new(180.0, 134.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(state.hour(), 3);
    nine_oclock_follows(&mut ui, *state);
}

#[test]
fn a_drag_to_another_hour_doesnt_lose_an_outside_set() {
    let state = twelve_hours(0, 0);
    let mut ui = time_picker(*state);
    ui.wait_for_idle();
    ui.on_all_nodes_unmerged(is_selectable_group()).on_last().perform_touch_input(|touch| {
        let (top, right) = (touch.top_center(), touch.center_right());
        touch.down(top);
        touch.move_to(right);
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(state.hour(), 3);
    nine_oclock_follows(&mut ui, *state);
}

#[test]
fn a_drag_onto_the_inner_circle_isnt_snapped_while_it_goes() {
    let state = twenty_four_hours(3, 0);
    let mut ui = time_picker(*state);
    ui.wait_for_idle();
    let mut group = ui.on_all_nodes_unmerged(is_selectable_group()).on_last();
    group.perform_touch_input(|touch| {
        touch.down(Offset::new(228.0, 128.0));
        // Past the slop, which the drag's offset leaves behind, then well into the inner circle
        // about 6 degrees past three: 15.
        touch.move_to(Offset::new(200.0, 128.0));
        touch.move_to(Offset::new(168.0, 134.0));
    });
    ui.wait_for_idle();
    assert_eq!(state.hour(), 15);
    assert!(state.current_angle().rem_euclid(2.0 * PI) > 0.05, "{}", state.current_angle());
    ui.on_all_nodes_unmerged(is_selectable_group()).on_last().perform_touch_input(|touch| {
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(state.hour(), 15);
    assert_angle(*state, 0.0);
}

#[test]
fn the_time_set_while_the_picker_was_away_is_where_the_dial_comes_back() {
    thread_local! {
        static SHOWN: Cell<Option<MutableState<bool>>> = const { Cell::new(None) };
    }
    let state = twelve_hours(3, 0);
    let handle = *state;
    let mut ui = compose_test(move || {
        let shown = remember(|| mutable_state_of(true));
        SHOWN.with(|cell| cell.set(Some(shown)));
        MaterialTheme!({
            if shown.get() {
                TimePicker!(handle);
            }
        });
    });
    let shown = SHOWN.with(Cell::get).unwrap();
    ui.wait_for_idle();
    // A tap that changes the hour, leaving as it goes: then the time moves without it.
    ui.on_all_nodes_with_content_description("6 o'clock").on_last().perform_click();
    shown.set(false);
    ui.wait_for_idle();
    state.set_selection(TimePickerSelectionMode::Hour);
    state.set_hour(9);
    ui.wait_for_idle();
    shown.set(true);
    ui.wait_for_idle();
    assert_angle(*state, PI);
    assert_eq!(state.hour(), 9);
}

#[test]
fn the_time_set_before_the_picker_starts_is_where_the_dial_starts() {
    let state = twelve_hours(3, 0);
    state.set_hour(9);
    let mut ui = time_picker(*state);
    ui.wait_for_idle();
    assert_angle(*state, PI);
}

#[test]
fn a_tap_while_the_dial_is_turning_itself_is_dropped() {
    let state = TimePickerState::new(3, 30, false, TimePickerSelectionMode::Minute);
    let mut ui = clock_face(*state, true);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.set_selection(TimePickerSelectionMode::Hour);
    ui.advance_time_by(std::time::Duration::from_millis(32));
    ui.on_all_nodes_unmerged(is_selectable_group()).on_last().perform_touch_input(|touch| {
        touch.click_at(Offset::new(128.0, 228.0));
    });
    ui.advance_time_by(std::time::Duration::from_millis(32));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(state.hour(), 3, "the tap never ran");
    assert_eq!(
        state.selection(),
        TimePickerSelectionMode::Hour,
        "and doesn't go on to the minutes"
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TimePickerTest.kt, timePicker_initialState
#[test]
fn twelve_hours_show_every_hour_on_their_face() {
    let shown = [
        "12", "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12", "01", "02",
        "03", "04", "05", "06", "07", "08", "09", "10", "11",
    ];
    for (hour, expected) in shown.iter().enumerate() {
        let state = twelve_hours(hour as i32, 59);
        let mut ui = time_picker(*state);
        ui.on_node_with_content_description("Select hour").assert(has_text(expected));
        drop(ui);
        let mut ui = time_input(*state);
        ui.on_node_with_content_description("for hour").assert(has_text(expected));
    }
}

fn minute_of_a_tap(degrees_from_the_top: f32) -> i32 {
    let state = TimePickerState::new(10, 23, false, TimePickerSelectionMode::Minute);
    let mut ui = clock_face(*state, false);
    ui.wait_for_idle();
    let radians = degrees_from_the_top.to_radians();
    let at = Offset::new(128.0 + 100.0 * radians.sin(), 128.0 - 100.0 * radians.cos());
    ui.on_all_nodes_unmerged(is_selectable_group())
        .on_last()
        .perform_touch_input(|touch| touch.click_at(at));
    ui.wait_for_idle();
    state.minute()
}

#[test]
fn a_tap_off_a_mark_goes_to_the_nearest_five_minutes() {
    assert_eq!(minute_of_a_tap(7.0), 0);
    assert_eq!(minute_of_a_tap(23.0), 5);
    assert_eq!(minute_of_a_tap(37.0), 5);
    assert_eq!(minute_of_a_tap(187.0), 30);
    assert_eq!(minute_of_a_tap(353.0), 0);
}
