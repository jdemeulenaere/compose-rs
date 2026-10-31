//! The time picker, as Compose's `TimePicker` and `TimeInput`: `TimePickerState`, the dial with its
//! drag and tap, the fields, `TimePickerDefaults`, `TimeInputDefaults`, their colors. Over fields
//! of a `String` until `TextFieldState` is there, in English (D1562); the differences are D1566.
//! `TimeScroll`, the vibrant variants and `TimePickerDialog` aren't here yet.

use std::cell::Cell;
use std::f32::consts::{FRAC_PI_2, PI};

use compose_animation::{Animatable, AnimationSpec, Crossfade, snap};
use compose_foundation::layout::{Arrangement, Box, Column, Row, Spacer};
use compose_foundation::{
    BasicTextField, CIRCLE_SHAPE, ImeAction, KeyboardActions, KeyboardOptions, KeyboardType,
    MutatePriority, MutatorMutex, RoundedCornerShape, VisualTransformation,
};
use compose_runtime::{
    ComposeArg, CompositionLocalProvider, LaunchedEffect, MutableState, ObjectHandle, Owned,
    OwnedState, composable, delay, derived_state_of, mutable_state_of, remember,
    remember_coroutine_scope, remember_saveable,
};
use compose_ui::{
    Alignment, Color, Constraints, Dp, DragGestures, FocusDirection, FocusRequester, FontWeight,
    HapticFeedbackType, InputMode, IntrinsicSize, Key, KeyEvent, KeyEventType, LOCAL_CONFIGURATION,
    LOCAL_DENSITY, LOCAL_FOCUS_MANAGER, LOCAL_HAPTIC_FEEDBACK, LOCAL_INPUT_MODE_MANAGER,
    LOCAL_LAYOUT_DIRECTION, LayoutDirection, LineHeightAlignment, LineHeightStyle, LineHeightTrim,
    LiveRegionMode, Locale, Measurable, MeasurePolicy, Modifier, MutableInteractionSource, Offset,
    PaddingValues, Paint, Rect, Role, TapGestures, TextAlign, TextStyle, measure_policy,
};

use crate::button::{ButtonColors, ButtonShapes, TextButton};
use crate::calendar_model::{default_locale, digit_value, is_date_digit, to_local_string};
use crate::components::{
    SelectableSurface, Text, TextFieldColors, minimum_interactive_component_size,
};
use crate::ripple::ripple;
use crate::text_field::{OutlinedTextFieldContainer, OutlinedTextFieldDecorationBox};
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

const MAX_HOUR_VALUE: i32 = 23;
const MAX_MINUTE_VALUE: i32 = 59;

const FULL_CIRCLE: f32 = 2.0 * PI;
const HALF_CIRCLE: f32 = PI;
const QUARTER_CIRCLE: f32 = FRAC_PI_2;
const RADIANS_PER_MINUTE: f32 = FULL_CIRCLE / 60.0;
const RADIANS_PER_HOUR: f32 = FULL_CIRCLE / 12.0;

const CLOCK_DIAL_CONTAINER_SIZE: Dp = Dp(256.0);
const CLOCK_DIAL_MID_CONTAINER_SIZE: Dp = Dp(238.0);
const CLOCK_DIAL_MIN_CONTAINER_SIZE: Dp = Dp(200.0);
const TIME_PICKER_MAX_HEIGHT: Dp = Dp(384.0);
const TIME_PICKER_MID_HEIGHT: Dp = Dp(330.0);
const SELECTOR_HANDLE_SIZE: Dp = Dp(48.0);
const SELECTOR_TRACK_WIDTH: Dp = Dp(2.0);
const SELECTOR_CENTER_SIZE: Dp = Dp(8.0);
const OUTER_CIRCLE_TO_SIZE_RATIO: f32 = 101.0 / 256.0;
const INNER_CIRCLE_TO_SIZE_RATIO: f32 = 69.0 / 256.0;
const MAX_DISTANCE: Dp = Dp(74.0);
const MINIMUM_INTERACTIVE_SIZE: Dp = Dp(48.0);
const CLOCK_DISPLAY_BOTTOM_MARGIN: Dp = Dp(36.0);
const CLOCK_FACE_BOTTOM_MARGIN: Dp = Dp(24.0);
const DISPLAY_SEPARATOR_WIDTH: Dp = Dp(24.0);
const SUPPORT_LABEL_TOP: Dp = Dp(7.0);
const PERIOD_TOGGLE_PADDING_SMALL: Dp = Dp(4.0);
const PERIOD_TOGGLE_PADDING_LARGE: Dp = Dp(16.0);
const PERIOD_SELECTOR_HORIZONTAL_WIDTH: Dp = Dp(216.0);
const PERIOD_SELECTOR_HORIZONTAL_HEIGHT: Dp = Dp(38.0);
const PERIOD_SELECTOR_VERTICAL_WIDTH: Dp = Dp(52.0);
const PERIOD_SELECTOR_VERTICAL_HEIGHT: Dp = Dp(80.0);
const TIME_SELECTOR_WIDTH: Dp = Dp(96.0);
const TIME_SELECTOR_HEIGHT: Dp = Dp(80.0);
const INPUT_PERIOD_SELECTOR_WIDTH: Dp = Dp(52.0);
const INPUT_PERIOD_SELECTOR_HEIGHT: Dp = Dp(72.0);
const TIME_FIELD_WIDTH: Dp = Dp(96.0);
const TIME_FIELD_HEIGHT: Dp = Dp(72.0);

/// Whether the hour or the minute is being picked, as Compose's `TimePickerSelectionMode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TimePickerSelectionMode {
    /// As `TimePickerSelectionMode.Hour`.
    Hour,
    /// As `TimePickerSelectionMode.Minute`.
    Minute,
}

impl ComposeArg for TimePickerSelectionMode {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// How a time picker is laid out, as Compose's `TimePickerLayoutType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TimePickerLayoutType {
    /// The display beside the dial: for landscape.
    Horizontal,
    /// The display over the dial: for portrait.
    Vertical,
}

impl ComposeArg for TimePickerLayoutType {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

struct TimePickerStateInner {
    is_24_hour: OwnedState<bool>,
    selection: OwnedState<TimePickerSelectionMode>,
    hour: OwnedState<i32>,
    minute: OwnedState<i32>,
    hour_input: OwnedState<i32>,
    minute_input: OwnedState<i32>,
    // What Compose's `AnalogTimePickerState` adds, for the dial (D1566).
    current_diameter: OwnedState<Dp>,
    dial_center: OwnedState<Offset>,
    dial_position: OwnedState<Offset>,
    is_dial_focusable: OwnedState<bool>,
    dial_focus_requester: FocusRequester,
    hour_node_focus_requester: FocusRequester,
    minute_node_focus_requester: FocusRequester,
    am_pm_node_focus_requester: FocusRequester,
    hour_angle: Cell<f32>,
    minute_angle: Cell<f32>,
    angle: Animatable<f32>,
    mutex: MutatorMutex,
    user_override: Cell<bool>,
    press: Cell<Offset>,
}

/// A time picker's state, as Compose's `TimePickerState`: make one with [`TimePickerState::new`],
/// or use [`remember_time_picker_state`]. `hour` and `minute` are always valid; `hour_input` and
/// `minute_input` follow what's typed, valid or not.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TimePickerState {
    object: ObjectHandle<TimePickerStateInner>,
}

fn hour_angle_of(hour: i32) -> f32 {
    RADIANS_PER_HOUR * (hour % 12) as f32 - FULL_CIRCLE / 4.0
}

fn minute_angle_of(minute: i32) -> f32 {
    RADIANS_PER_MINUTE * minute as f32 - FULL_CIRCLE / 4.0
}

/// The hour an angle of the dial is on, as `Float.toHour`.
fn to_hour(angle: f32) -> i32 {
    let total_offset = RADIANS_PER_HOUR / 2.0 + QUARTER_CIRCLE;
    ((angle + total_offset) / RADIANS_PER_HOUR) as i32 % 12
}

/// The minute an angle of the dial is on, as `Float.toMinute`.
fn to_minute(angle: f32) -> i32 {
    let total_offset = RADIANS_PER_MINUTE / 2.0 + QUARTER_CIRCLE;
    ((angle + total_offset) / RADIANS_PER_MINUTE) as i32 % 60
}

fn offset_angle(angle: f32) -> f32 {
    let offset = angle + QUARTER_CIRCLE;
    if offset < 0.0 { offset + FULL_CIRCLE } else { offset }
}

fn normalize(angle: f32) -> f32 {
    let normalized = f64::from(angle) % (2.0 * std::f64::consts::PI);
    (if normalized < 0.0 { normalized + 2.0 * std::f64::consts::PI } else { normalized }) as f32
}

/// The angle of a point from a center, 0 being at six o'clock's opposite as Compose's `atan`.
fn atan(y: f32, x: f32) -> f32 {
    let angle = y.atan2(x) - QUARTER_CIRCLE;
    if angle < 0.0 { angle + FULL_CIRCLE } else { angle }
}

impl TimePickerStateInner {
    fn is_pm(&self) -> bool {
        self.hour.get() >= 12
    }

    fn is_valid_hour(&self, hour: i32) -> bool {
        if self.is_24_hour.get() {
            (0..=MAX_HOUR_VALUE).contains(&hour)
        } else if self.is_pm() {
            (12..=MAX_HOUR_VALUE).contains(&hour)
        } else {
            (0..=11).contains(&hour)
        }
    }

    /// The end of an animation from the current angle to `new`, the short way round.
    fn end_value_for_animation(&self, new: f32) -> f32 {
        let value = self.angle.value();
        let mut difference = value - new;
        while difference > HALF_CIRCLE {
            difference -= FULL_CIRCLE;
        }
        while difference <= -HALF_CIRCLE {
            difference += FULL_CIRCLE;
        }
        value - difference
    }

    fn selected_angle(&self) -> f32 {
        match self.selection.get() {
            TimePickerSelectionMode::Hour => self.hour_angle.get(),
            TimePickerSelectionMode::Minute => self.minute_angle.get(),
        }
    }

    /// The dial's angles from the time: it follows a time set from outside.
    fn follow_time(&self) {
        self.hour_angle.set(hour_angle_of(self.hour.get()));
        self.minute_angle.set(minute_angle_of(self.minute.get()));
        self.angle.snap_to(self.selected_angle());
    }
}

impl TimePickerState {
    /// Its object, or `None` once its owner went (D240): then it does nothing.
    #[track_caller]
    fn inner(&self) -> Option<std::rc::Rc<TimePickerStateInner>> {
        self.object.get()
    }

    /// As `TimePickerState(initialHour, initialMinute, is24Hour, initialSelection)`.
    ///
    /// # Panics
    ///
    /// If `initial_hour` isn't in 0 to 23 or `initial_minute` in 0 to 59, as Compose's `require`.
    #[track_caller]
    pub fn new(
        initial_hour: i32,
        initial_minute: i32,
        is_24_hour: bool,
        initial_selection: TimePickerSelectionMode,
    ) -> Owned<TimePickerState> {
        assert!((0..=MAX_HOUR_VALUE).contains(&initial_hour), "initial_hour should be in 0..=23");
        assert!(
            (0..=MAX_MINUTE_VALUE).contains(&initial_minute),
            "initial_minute should be in 0..=59"
        );
        let (hour_angle, minute_angle) =
            (hour_angle_of(initial_hour), minute_angle_of(initial_minute));
        let angle = match initial_selection {
            TimePickerSelectionMode::Hour => hour_angle,
            TimePickerSelectionMode::Minute => minute_angle,
        };
        Owned::new(
            TimePickerStateInner {
                is_24_hour: mutable_state_of(is_24_hour),
                selection: mutable_state_of(initial_selection),
                hour: mutable_state_of(initial_hour),
                minute: mutable_state_of(initial_minute),
                hour_input: mutable_state_of(initial_hour),
                minute_input: mutable_state_of(initial_minute),
                current_diameter: mutable_state_of(Dp(0.0)),
                dial_center: mutable_state_of(Offset::ZERO),
                dial_position: mutable_state_of(Offset::ZERO),
                is_dial_focusable: mutable_state_of(false),
                dial_focus_requester: FocusRequester::new(),
                hour_node_focus_requester: FocusRequester::new(),
                minute_node_focus_requester: FocusRequester::new(),
                am_pm_node_focus_requester: FocusRequester::new(),
                hour_angle: Cell::new(hour_angle),
                minute_angle: Cell::new(minute_angle),
                angle: Animatable::new(angle),
                mutex: MutatorMutex::new(),
                user_override: Cell::new(false),
                press: Cell::new(Offset::ZERO),
            },
            |object| TimePickerState { object },
        )
    }

    /// The selected hour, 0 to 23, as `hour`.
    pub fn hour(&self) -> i32 {
        self.inner().map_or(0, |inner| inner.hour.get())
    }

    /// As setting `hour`.
    pub fn set_hour(&self, hour: i32) {
        if let Some(inner) = self.inner() {
            inner.hour.set(hour);
            inner.hour_input.set(hour);
        }
    }

    /// The selected minute, 0 to 59, as `minute`.
    pub fn minute(&self) -> i32 {
        self.inner().map_or(0, |inner| inner.minute.get())
    }

    /// As setting `minute`.
    pub fn set_minute(&self, minute: i32) {
        if let Some(inner) = self.inner() {
            inner.minute.set(minute);
            inner.minute_input.set(minute);
        }
    }

    /// What's entered for the hour, maybe invalid, as `hourInput`.
    pub fn hour_input(&self) -> i32 {
        self.inner().map_or(0, |inner| inner.hour_input.get())
    }

    /// As setting `hourInput`: a valid one is the hour too, and the dial goes there.
    pub fn set_hour_input(&self, value: i32) {
        if let Some(inner) = self.inner() {
            if inner.is_valid_hour(value) {
                inner.hour.set(value);
                inner.hour_angle.set(hour_angle_of(value));
                if inner.selection.get() == TimePickerSelectionMode::Hour {
                    inner.angle.snap_to(inner.hour_angle.get());
                }
            }
            inner.hour_input.set(value);
        }
    }

    /// What's entered for the minute, maybe invalid, as `minuteInput`.
    pub fn minute_input(&self) -> i32 {
        self.inner().map_or(0, |inner| inner.minute_input.get())
    }

    /// As setting `minuteInput`: a valid one is the minute too, and the dial goes there.
    pub fn set_minute_input(&self, value: i32) {
        if let Some(inner) = self.inner() {
            if (0..=MAX_MINUTE_VALUE).contains(&value) {
                inner.minute.set(value);
                inner.minute_angle.set(minute_angle_of(value));
                if inner.selection.get() == TimePickerSelectionMode::Minute {
                    inner.angle.snap_to(inner.minute_angle.get());
                }
            }
            inner.minute_input.set(value);
        }
    }

    /// As `is24hour`.
    pub fn is_24_hour(&self) -> bool {
        self.inner().is_some_and(|inner| inner.is_24_hour.get())
    }

    /// As setting `is24hour`.
    pub fn set_is_24_hour(&self, is_24_hour: bool) {
        if let Some(inner) = self.inner() {
            inner.is_24_hour.set(is_24_hour);
        }
    }

    /// As `selection`.
    pub fn selection(&self) -> TimePickerSelectionMode {
        self.inner().map_or(TimePickerSelectionMode::Hour, |inner| inner.selection.get())
    }

    /// As setting `selection`.
    pub fn set_selection(&self, selection: TimePickerSelectionMode) {
        if let Some(inner) = self.inner() {
            inner.selection.set(selection);
        }
    }

    /// From noon on, as `isPm`.
    pub fn is_pm(&self) -> bool {
        self.inner().is_some_and(|inner| inner.is_pm())
    }

    /// As `isHourInputValid`: 0 to 23, or the period's half.
    pub fn is_hour_input_valid(&self) -> bool {
        self.inner().is_none_or(|inner| inner.is_valid_hour(inner.hour_input.get()))
    }

    /// As `isMinuteInputValid`.
    pub fn is_minute_input_valid(&self) -> bool {
        (0..=MAX_MINUTE_VALUE).contains(&self.minute_input())
    }

    /// As `isInputValid`.
    pub fn is_input_valid(&self) -> bool {
        self.is_minute_input_valid() && self.is_hour_input_valid()
    }

    /// The hour on a twelve or twenty-four hour clock's face, as `hourForDisplay`.
    fn hour_for_display(&self) -> i32 {
        let hour = self.hour();
        match () {
            _ if self.is_24_hour() => hour % 24,
            _ if hour % 12 == 0 => 12,
            _ if self.is_pm() => hour - 12,
            _ => hour,
        }
    }

    /// The dial's angle, as `currentAngle`: for the tests, as the internal one.
    #[doc(hidden)]
    pub fn current_angle(&self) -> f32 {
        self.inner().map_or(0.0, |inner| inner.angle.value())
    }

    /// As setting `isDialFocusable`: for the tests, as the internal one.
    #[doc(hidden)]
    pub fn set_is_dial_focusable(&self, focusable: bool) {
        if let Some(inner) = self.inner() {
            inner.is_dial_focusable.set(focusable);
        }
    }

    fn is_dial_focusable(&self) -> bool {
        self.inner().is_some_and(|inner| inner.is_dial_focusable.get())
    }

    /// As `animateToCurrent`: nothing when it's there.
    async fn animate_to_current(self, spec: AnimationSpec) {
        let target = self.inner().map(|inner| {
            normalize(inner.angle.target_value()) == normalize(inner.selected_angle())
        });
        if target == Some(false) {
            self.on_gesture_end(spec).await;
        }
    }

    /// As `onGestureEnd`: a drag ends on the value it was over.
    async fn on_gesture_end(self, spec: AnimationSpec) {
        let Some(inner) = self.inner() else { return };
        let end = inner.end_value_for_animation(inner.selected_angle());
        let _ = inner
            .mutex
            .mutate(MutatePriority::PreventUserInput, async {
                inner.angle.animate_to(end, spec).await;
            })
            .await;
    }

    /// As `rotateTo`: the value under an angle, and the dial there.
    async fn rotate_to(self, angle: f32, spec: AnimationSpec, animate: bool) {
        let Some(inner) = self.inner() else { return };
        inner.user_override.set(false);
        let _ = inner
            .mutex
            .mutate(MutatePriority::UserInput, async {
                if inner.selection.get() == TimePickerSelectionMode::Hour {
                    let hour_angle = (to_hour(angle) % 12) as f32 * RADIANS_PER_HOUR;
                    inner.hour_angle.set(hour_angle);
                    let hour = to_hour(hour_angle) % 12 + if inner.is_pm() { 12 } else { 0 };
                    if inner.is_valid_hour(hour) {
                        inner.hour.set(hour);
                    }
                    inner.hour_input.set(hour);
                } else {
                    let minute_angle = to_minute(angle) as f32 * RADIANS_PER_MINUTE;
                    inner.minute_angle.set(minute_angle);
                    let minute = to_minute(minute_angle);
                    inner.minute.set(minute);
                    inner.minute_input.set(minute);
                }
                if animate {
                    let end = inner.end_value_for_animation(offset_angle(angle));
                    inner.angle.animate_to(end, spec).await;
                } else {
                    inner.angle.snap_to(offset_angle(angle));
                }
            })
            .await;
    }

    /// As `moveSelector`: with twenty-four hours, the inner circle is the afternoon's.
    fn move_selector(self, x: f32, y: f32, max_distance: f32, center: Offset) {
        if self.selection() == TimePickerSelectionMode::Hour && self.is_24_hour() {
            let distance = (center.x - x).hypot(center.y - y);
            if self.is_pm() {
                if distance >= max_distance {
                    self.set_hour(self.hour() - 12);
                }
            } else if distance < max_distance {
                self.set_hour(self.hour() + 12);
            }
        }
    }

    /// As `onTap`: rounds to the hours or the five minutes, then goes on to the minutes.
    async fn on_tap(
        self,
        x: f32,
        y: f32,
        max_distance: f32,
        auto_switch_to_minute: bool,
        center: Offset,
        spec: AnimationSpec,
    ) {
        let mut angle = atan(y - center.y, x - center.x);
        let hours = self.selection() == TimePickerSelectionMode::Hour;
        angle = if hours {
            (angle / RADIANS_PER_HOUR).round() * RADIANS_PER_HOUR
        } else {
            (angle / RADIANS_PER_MINUTE / 5.0).round() * 5.0 * RADIANS_PER_MINUTE
        };
        self.move_selector(x, y, max_distance, center);
        self.rotate_to(angle, spec, true).await;
        if hours && auto_switch_to_minute {
            delay(std::time::Duration::from_millis(100)).await;
        }
        if auto_switch_to_minute {
            self.set_selection(TimePickerSelectionMode::Minute);
        }
    }

    /// Where the selector's handle is on the dial, as `selectorPos`, in dp.
    fn selector_position(&self) -> Offset {
        let Some(inner) = self.inner() else { return Offset::ZERO };
        let diameter = inner.current_diameter.get().0;
        let handle_radius = SELECTOR_HANDLE_SIZE.0 / 2.0 * diameter / CLOCK_DIAL_CONTAINER_SIZE.0;
        let on_inner_circle =
            self.is_24_hour() && self.is_pm() && self.selection() == TimePickerSelectionMode::Hour;
        let ratio =
            if on_inner_circle { INNER_CIRCLE_TO_SIZE_RATIO } else { OUTER_CIRCLE_TO_SIZE_RATIO };
        let length = (diameter * ratio - handle_radius).max(0.0) + handle_radius;
        let angle = inner.angle.value();
        Offset::new(length * angle.cos() + diameter / 2.0, length * angle.sin() + diameter / 2.0)
    }

    /// What's saved of a state, as `TimePickerStateImpl.Saver`.
    fn saver() -> impl compose_runtime::Saver<Owned<TimePickerState>> {
        use compose_runtime::Saveable;
        compose_runtime::saver(
            |state: &Owned<TimePickerState>| {
                state.inner()?;
                let minute = i32::from(state.selection() == TimePickerSelectionMode::Minute);
                Some((state.hour(), state.minute(), state.is_24_hour(), minute).save())
            },
            |saved| {
                <(i32, i32, bool, i32)>::restore(saved).map(|(hour, minute, is_24_hour, mode)| {
                    let selection = if mode == 1 {
                        TimePickerSelectionMode::Minute
                    } else {
                        TimePickerSelectionMode::Hour
                    };
                    TimePickerState::new(hour, minute, is_24_hour, selection)
                })
            },
        )
    }
}

impl ComposeArg for TimePickerState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Whether a locale's clock has twenty-four hours, as Compose's `is24HourFormat` for a phone left
/// at its locale's default: a user's choice in the system's settings isn't read (D1566). Hidden: a
/// default of `remember_time_picker_state`.
#[doc(hidden)]
pub fn is_24_hour_format() -> bool {
    locale_uses_24_hours(&default_locale())
}

/// The regions that prefer twelve hours, by CLDR's `timeData` from memory, as the date tables.
fn locale_uses_24_hours(locale: &Locale) -> bool {
    let region = locale.region();
    let language = locale.to_language_tag();
    let language = language.split('-').next().unwrap_or_default().to_ascii_lowercase();
    let twelve_hours = match region.as_str() {
        "" => matches!(language.as_str(), "en" | "ko" | "ar" | "hi"),
        "US" | "CA" | "AU" | "NZ" | "IN" | "PK" | "BD" | "PH" | "KR" | "EG" | "SA" | "JO"
        | "IQ" | "KW" | "QA" | "BH" | "OM" | "AE" | "YE" | "LB" | "SY" | "SD" | "MY" | "CO"
        | "VE" => true,
        _ => false,
    };
    !twelve_hours
}

/// A time picker state remembered here and saved, as `rememberTimePickerState`: `is_24_hour` is
/// the locale's by default.
#[composable(non_restartable)]
pub fn remember_time_picker_state(
    #[default(0)] initial_hour: i32,
    #[default(0)] initial_minute: i32,
    #[default(crate::is_24_hour_format())] is_24_hour: bool,
) -> TimePickerState {
    compose_runtime::remember_saveable_with(TimePickerState::saver(), move || {
        TimePickerState::new(
            initial_hour,
            initial_minute,
            is_24_hour,
            TimePickerSelectionMode::Hour,
        )
    })
}

/// A time picker's colors, as Compose's `TimePickerColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TimePickerColors {
    pub clock_dial_color: Color,
    pub selector_color: Color,
    pub container_color: Color,
    pub period_selector_border_color: Color,
    pub clock_dial_selected_content_color: Color,
    pub clock_dial_content_color: Color,
    pub period_selector_selected_container_color: Color,
    pub period_selector_container_color: Color,
    pub period_selector_selected_content_color: Color,
    pub period_selector_content_color: Color,
    pub time_selector_selected_container_color: Color,
    pub time_selector_container_color: Color,
    pub time_selector_selected_content_color: Color,
    pub time_selector_content_color: Color,
}

impl ComposeArg for TimePickerColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A time input's colors, as Compose's `TimeInputColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TimeInputColors {
    pub container_color: Color,
    pub period_selector_border_color: Color,
    pub period_selector_selected_container_color: Color,
    pub period_selector_container_color: Color,
    pub period_selector_selected_content_color: Color,
    pub period_selector_content_color: Color,
    pub time_text_field_colors: TextFieldColors,
}

impl ComposeArg for TimeInputColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl TimeInputColors {
    fn period_selector_container_color_of(&self, selected: bool) -> Color {
        if selected {
            self.period_selector_selected_container_color
        } else {
            self.period_selector_container_color
        }
    }

    fn period_selector_content_color_of(&self, selected: bool) -> Color {
        if selected {
            self.period_selector_selected_content_color
        } else {
            self.period_selector_content_color
        }
    }

    fn time_selector_container_color_of(&self, selected: bool) -> Color {
        let fields = &self.time_text_field_colors;
        if selected { fields.focused_container_color } else { fields.unfocused_container_color }
    }

    fn time_selector_content_color_of(&self, selected: bool) -> Color {
        let fields = &self.time_text_field_colors;
        if selected { fields.focused_text_color } else { fields.unfocused_text_color }
    }
}

impl TimePickerColors {
    /// As `toTimeInputColors`.
    fn to_time_input_colors(self) -> TimeInputColors {
        TimeInputColors {
            container_color: self.container_color,
            period_selector_border_color: self.period_selector_border_color,
            period_selector_selected_container_color: self.period_selector_selected_container_color,
            period_selector_container_color: self.period_selector_container_color,
            period_selector_selected_content_color: self.period_selector_selected_content_color,
            period_selector_content_color: self.period_selector_content_color,
            time_text_field_colors: TextFieldColors {
                focused_container_color: self.time_selector_selected_container_color,
                unfocused_container_color: self.time_selector_container_color,
                focused_text_color: self.time_selector_selected_content_color,
                unfocused_text_color: self.time_selector_content_color,
                ..crate::OutlinedTextFieldDefaults::colors()
            },
        }
    }
}

/// A time picker's defaults, as Compose's `TimePickerDefaults`.
pub struct TimePickerDefaults;

impl TimePickerDefaults {
    /// As `colors()`, with the tip's flag for the updated toggle on, its default: change them with
    /// struct update.
    pub fn colors() -> TimePickerColors {
        let scheme = MaterialTheme::color_scheme();
        TimePickerColors {
            clock_dial_color: scheme.surface_container_highest,
            clock_dial_selected_content_color: scheme.on_primary,
            clock_dial_content_color: scheme.on_surface,
            selector_color: scheme.primary,
            container_color: scheme.surface_container_high,
            period_selector_border_color: scheme.outline,
            period_selector_selected_container_color: scheme.primary_container,
            period_selector_container_color: scheme.surface_container_lowest,
            period_selector_selected_content_color: scheme.on_primary_container,
            period_selector_content_color: scheme.on_surface_variant,
            time_selector_selected_container_color: scheme.primary_container,
            time_selector_container_color: scheme.surface_container_highest,
            time_selector_selected_content_color: scheme.on_primary_container,
            time_selector_content_color: scheme.on_surface,
        }
    }

    /// As `layoutType()`: by the screen's shape.
    pub fn layout_type() -> TimePickerLayoutType {
        let configuration = LOCAL_CONFIGURATION.current();
        if configuration.screen_height_dp < configuration.screen_width_dp {
            TimePickerLayoutType::Horizontal
        } else {
            TimePickerLayoutType::Vertical
        }
    }
}

/// A time input's defaults, as Compose's `TimeInputDefaults`.
pub struct TimeInputDefaults;

impl TimeInputDefaults {
    /// As `colors()`, with the tip's flag for the updated toggle on: change them with struct
    /// update.
    pub fn colors() -> TimeInputColors {
        let scheme = MaterialTheme::color_scheme();
        TimeInputColors {
            container_color: scheme.surface_container_high,
            period_selector_border_color: scheme.outline,
            period_selector_selected_container_color: scheme.primary_container,
            period_selector_container_color: scheme.surface_container_lowest,
            period_selector_selected_content_color: scheme.on_primary_container,
            period_selector_content_color: scheme.on_surface_variant,
            time_text_field_colors: TextFieldColors {
                focused_container_color: scheme.primary_container,
                unfocused_container_color: scheme.surface_container_highest,
                focused_text_color: scheme.on_primary_container,
                unfocused_text_color: scheme.on_surface,
                focused_indicator_color: scheme.outline,
                unfocused_indicator_color: scheme.outline,
                error_container_color: scheme.error_container,
                error_indicator_color: scheme.error,
                ..crate::OutlinedTextFieldDefaults::colors()
            },
        }
    }
}

/// A time picker, as Compose's `TimePicker`: the time and the period over or beside a dial.
#[composable]
pub fn TimePicker(
    state: TimePickerState,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::TimePickerDefaults::colors())] colors: TimePickerColors,
    #[default(crate::TimePickerDefaults::layout_type())] layout_type: TimePickerLayoutType,
) {
    // No accessibility services are known of here: only a keyboard stays on the hours (D1566).
    let auto_switch = LOCAL_INPUT_MODE_MANAGER.current().input_mode() != InputMode::Keyboard;
    LaunchedEffect((state.hour(), state.minute()), async move {
        if let Some(inner) = state.inner() {
            if inner.user_override.get() {
                inner.follow_time();
            }
            inner.user_override.set(true);
        }
    });
    match layout_type {
        TimePickerLayoutType::Vertical => {
            Column!(
                modifier = modifier.clone().semantics(|config| config.set_is_traversal_group(true)),
                horizontal_alignment = Alignment::CENTER_HORIZONTALLY,
                {
                    VerticalClockDisplay!(state, colors);
                    Spacer!(modifier = Modifier.height(CLOCK_DISPLAY_BOTTOM_MARGIN));
                    ClockFace!(
                        Modifier.size(CLOCK_DIAL_CONTAINER_SIZE),
                        state,
                        colors,
                        auto_switch
                    );
                    Spacer!(modifier = Modifier.height(CLOCK_FACE_BOTTOM_MARGIN));
                }
            );
        }
        TimePickerLayoutType::Horizontal => {
            Row!(
                modifier = modifier.clone().semantics(|config| config.set_is_traversal_group(true)),
                vertical_alignment = Alignment::CENTER_VERTICALLY,
                {
                    HorizontalClockDisplay!(state, colors);
                    Spacer!(modifier = Modifier.width(CLOCK_DISPLAY_BOTTOM_MARGIN));
                    ClockFaceSized!({
                        ClockFace!(Modifier.fill_max_size(), state, colors, auto_switch);
                    });
                }
            );
        }
    }
}

/// The dial's size by the height it's given, as Compose's `ClockFaceSizeModifier`.
#[composable]
fn ClockFaceSized(content: impl Fn() + 'static) {
    compose_ui::Layout(
        Modifier,
        measure_policy(|scope, measurables: &[Measurable], constraints: Constraints| {
            let density = scope.density();
            let maximum = constraints.max_height as f32 / density.density;
            let size = if maximum >= TIME_PICKER_MAX_HEIGHT.0 {
                CLOCK_DIAL_CONTAINER_SIZE
            } else if maximum >= TIME_PICKER_MID_HEIGHT.0 {
                CLOCK_DIAL_MID_CONTAINER_SIZE
            } else {
                CLOCK_DIAL_MIN_CONTAINER_SIZE
            };
            let size = density.round_to_px(size);
            let child = scope.measure(measurables[0], Constraints::fixed(size, size));
            scope.place(&child, 0, 0);
            scope.layout(size, size)
        }),
        content,
    );
}

/// As Compose's `HorizontalClockDisplay`.
#[composable]
fn HorizontalClockDisplay(state: TimePickerState, colors: TimePickerColors) {
    Column!(vertical_arrangement = Arrangement::CENTER, {
        ClockDisplayNumbers!(state, colors);
        if !state.is_24_hour() {
            Box!(
                modifier =
                    Modifier.padding((Dp(0.0), PERIOD_TOGGLE_PADDING_LARGE, Dp(0.0), Dp(0.0))),
                {
                    PeriodToggle!(
                        Modifier.size((
                            PERIOD_SELECTOR_HORIZONTAL_WIDTH,
                            PERIOD_SELECTOR_HORIZONTAL_HEIGHT
                        )),
                        state,
                        colors.to_time_input_colors(),
                        true
                    );
                }
            );
        }
    });
}

/// As Compose's `VerticalClockDisplay`.
#[composable]
fn VerticalClockDisplay(state: TimePickerState, colors: TimePickerColors) {
    Row!(horizontal_arrangement = Arrangement::CENTER, {
        ClockDisplayNumbers!(state, colors);
        if !state.is_24_hour() {
            Box!(
                modifier =
                    Modifier.padding((PERIOD_TOGGLE_PADDING_SMALL, Dp(0.0), Dp(0.0), Dp(0.0))),
                {
                    PeriodToggle!(
                        Modifier.size((
                            PERIOD_SELECTOR_VERTICAL_WIDTH,
                            PERIOD_SELECTOR_VERTICAL_HEIGHT
                        )),
                        state,
                        colors.to_time_input_colors(),
                        false
                    );
                }
            );
        }
    });
}

/// As Compose's `ClockDisplayNumbers`: always from left to right.
#[composable]
fn ClockDisplayNumbers(state: TimePickerState, colors: TimePickerColors) {
    let input_colors = colors.to_time_input_colors();
    let on_focus_changed = move |focus: compose_ui::FocusState| {
        if focus.is_focused() {
            state.set_is_dial_focusable(false);
        }
    };
    let on_activated = move || state.set_is_dial_focusable(true);
    let Some(inner) = state.inner() else { return };
    let (hour_requester, minute_requester, dial) = (
        inner.hour_node_focus_requester,
        inner.minute_node_focus_requester,
        inner.dial_focus_requester,
    );
    let style = MaterialTheme::typography().display_large.clone();
    CompositionLocalProvider(
        (LOCAL_TEXT_STYLE.provides(style), LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Ltr)),
        || {
            Row!({
                TimeSelector!(
                    Modifier
                        .size((TIME_SELECTOR_WIDTH, TIME_SELECTOR_HEIGHT))
                        .on_focus_changed(on_focus_changed)
                        .focus_requester(hour_requester),
                    state.hour_for_display(),
                    state,
                    TimePickerSelectionMode::Hour,
                    input_colors,
                    true,
                    on_activated
                );
                DisplaySeparator!(
                    Modifier.size((DISPLAY_SEPARATOR_WIDTH, PERIOD_SELECTOR_VERTICAL_HEIGHT))
                );
                TimeSelector!(
                    Modifier
                        .size((TIME_SELECTOR_WIDTH, TIME_SELECTOR_HEIGHT))
                        .on_focus_changed(on_focus_changed)
                        .focus_properties(move |properties| {
                            if state.is_dial_focusable() {
                                properties.down = dial;
                            }
                        })
                        .focus_requester(minute_requester),
                    state.minute(),
                    state,
                    TimePickerSelectionMode::Minute,
                    input_colors,
                    true,
                    on_activated
                );
            });
        },
    );
}

/// As Compose's `HorizontalPeriodToggle` and `VerticalPeriodToggle`, with the tip's flag for the
/// updated toggle on: two buttons with a gap. `ToggleButton` isn't here yet: `TextButton`s with
/// its colors and shapes (D1566).
#[composable]
fn PeriodToggle(
    modifier: Modifier,
    state: TimePickerState,
    colors: TimeInputColors,
    horizontal: bool,
) {
    let policy = remember(move || {
        measure_policy(move |scope, measurables: &[Measurable], constraints: Constraints| {
            let gap = scope.density().round_to_px(PERIOD_TOGGLE_PADDING_SMALL);
            let (width, height) = if horizontal {
                (((constraints.max_width - gap) / 2).max(0), constraints.max_height)
            } else {
                (constraints.max_width, ((constraints.max_height - gap) / 2).max(0))
            };
            let mut at = 0;
            for &measurable in measurables {
                let item = scope.measure(measurable, Constraints::new(0, width, 0, height));
                let (x, y) = if horizontal { (at, 0) } else { (0, at) };
                scope.place(&item, x, y);
                at += gap + if horizontal { item.width } else { item.height };
            }
            scope.layout(constraints.max_width, constraints.max_height)
        })
    });
    let on_focus_changed = move |focus: compose_ui::FocusState| {
        if focus.is_focused() {
            state.set_is_dial_focusable(false);
        }
    };
    let requester = state.inner().map(|inner| inner.am_pm_node_focus_requester);
    let requester_modifier = requester.map_or(Modifier, |it| Modifier.focus_requester(it));
    compose_ui::Layout(
        modifier
            .on_focus_changed(on_focus_changed)
            .semantics(|config| {
                config.set_is_traversal_group(true);
                config.set_content_description("Select AM or PM");
            })
            .selectable_group()
            .then(requester_modifier),
        policy,
        compose_runtime::callback(move || {
            ToggleItem!(
                !state.is_pm(),
                move || {
                    if state.is_pm() && state.is_hour_input_valid() {
                        state.set_hour(state.hour() - 12);
                    }
                },
                colors,
                "AM"
            );
            ToggleItem!(
                state.is_pm(),
                move || {
                    if !state.is_pm() && state.is_hour_input_valid() {
                        state.set_hour(state.hour() + 12);
                    }
                },
                colors,
                "PM"
            );
        }),
    );
}

/// As Compose's `ToggleItem` with its `ToggleButton`'s colors and shapes.
#[composable]
fn ToggleItem(
    checked: bool,
    on_click: impl Fn() + 'static,
    colors: TimeInputColors,
    text: &'static str,
) {
    let rounded = RoundedCornerShape(Dp(12.0));
    let shape = if checked { rounded.clone() } else { CIRCLE_SHAPE };
    let style = MaterialTheme::typography().title_medium.clone();
    let style =
        if checked { TextStyle { font_weight: Some(FontWeight::BOLD), ..style } } else { style };
    TextButton!(
        on_click,
        modifier = Modifier
            .z_index(if checked { 0.0 } else { 1.0 })
            .fill_max_size()
            .semantics(move |config| config.set_selected(checked)),
        shapes = ButtonShapes { shape, pressed_shape: rounded },
        colors = ButtonColors {
            container_color: colors.period_selector_container_color_of(checked),
            content_color: colors.period_selector_content_color_of(checked),
            ..crate::ButtonDefaults::text_button_colors()
        },
        content_padding = PaddingValues::from((PERIOD_TOGGLE_PADDING_SMALL, Dp(0.0))),
        {
            Text!("{text}", style = style.clone(), max_lines = 1);
        }
    );
}

/// As Compose's `DisplaySeparator`.
#[composable]
fn DisplaySeparator(modifier: Modifier) {
    let style = TextStyle {
        text_align: Some(TextAlign::Center),
        line_height_style: Some(LineHeightStyle {
            alignment: LineHeightAlignment::CENTER,
            trim: LineHeightTrim::Both,
            ..LineHeightStyle::DEFAULT
        }),
        ..LOCAL_TEXT_STYLE.current()
    };
    Box!(
        modifier = modifier.clear_and_set_semantics(|_| {}),
        content_alignment = Alignment::CENTER,
        {
            Text!(
                ":",
                modifier = Modifier.offset(Dp(0.0), Dp(-4.0)),
                color = MaterialTheme::color_scheme().on_surface,
                style = style.clone()
            );
        }
    );
}

/// What a number is read out as, as `numberContentDescription`.
fn number_content_description(
    selection: TimePickerSelectionMode,
    is_24_hour: bool,
    number: i32,
) -> String {
    match selection {
        TimePickerSelectionMode::Minute => format!("{number} minutes"),
        TimePickerSelectionMode::Hour if is_24_hour => format!("{number} hours"),
        TimePickerSelectionMode::Hour => format!("{number} o'clock"),
    }
}

/// As Compose's `TimeSelector`.
#[composable]
#[allow(clippy::too_many_arguments)]
fn TimeSelector(
    modifier: Modifier,
    value: i32,
    state: TimePickerState,
    selection: TimePickerSelectionMode,
    colors: TimeInputColors,
    is_valid: bool,
    on_selector_activated: impl Fn() + 'static,
) {
    let selected = state.selection() == selection;
    let scheme = MaterialTheme::color_scheme();
    let description = match selection {
        TimePickerSelectionMode::Hour => "Select hour",
        TimePickerSelectionMode::Minute => "Select minutes",
    };
    let container_color = if is_valid {
        colors.time_selector_container_color_of(selected)
    } else {
        scheme.error_container
    };
    let content_color = if is_valid {
        colors.time_selector_content_color_of(selected)
    } else {
        scheme.on_error_container
    };
    let value_description = number_content_description(selection, state.is_24_hour(), value);
    let text = to_local_string(value, 2, 40, &default_locale());
    SelectableSurface!(
        selected,
        move || {
            if selection != state.selection() {
                state.set_selection(selection);
            }
            on_selector_activated();
        },
        modifier = modifier.semantics_with(true, move |config| {
            config.set_role(Role::RadioButton);
            config.set_content_description(description);
        }),
        shape = MaterialTheme::shapes().small.clone(),
        color = container_color,
        {
            Box!(content_alignment = Alignment::CENTER, {
                let value_description = value_description.clone();
                Text!(
                    "{text}",
                    modifier = Modifier.semantics(move |config| {
                        config.set_content_description(value_description.clone());
                    }),
                    color = content_color
                );
            });
        }
    );
}

const MINUTES: [i32; 12] = [0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55];
const HOURS: [i32; 12] = [12, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

/// The dial, as Compose's `ClockFace` and its `ClockDialModifier`: hidden, for the tests, as the
/// internal one. The numbers are drawn over the selector, in the selected color when it's over
/// them: Compose's blending shows only their part under it (D1566).
#[doc(hidden)]
#[composable]
pub fn ClockFace(
    modifier: Modifier,
    state: TimePickerState,
    colors: TimePickerColors,
    auto_switch_to_minute: bool,
) {
    let Some(inner) = state.inner() else { return };
    let focus_manager = LOCAL_FOCUS_MANAGER.current();
    let first_outer = remember(FocusRequester::new);
    let last_outer = remember(FocusRequester::new);
    let first_inner = remember(FocusRequester::new);
    let last_inner = remember(FocusRequester::new);
    let spec = MaterialTheme::motion_scheme().default_spatial_spec();
    let coroutine_scope = remember_coroutine_scope();
    let selection_now = state.selection();
    {
        let spec = spec.clone();
        LaunchedEffect(selection_now, async move { state.animate_to_current(spec).await });
    }
    let max_distance_on_dial = {
        let inner = inner.clone();
        let density = LOCAL_DENSITY.current();
        move || {
            density.to_px(MAX_DISTANCE) * density.round_to_px(inner.current_diameter.get()) as f32
                / density.round_to_px(CLOCK_DIAL_CONTAINER_SIZE) as f32
        }
    };
    let size_density = LOCAL_DENSITY.current();
    let tap_spec = spec.clone();
    let drag_spec = spec.clone();
    let (tap_focus, press_focus) = (focus_manager.clone(), focus_manager.clone());
    let (tap_distance, drag_distance) = (max_distance_on_dial.clone(), max_distance_on_dial);
    Crossfade!(
        selection_now,
        modifier = modifier
            .background_with(colors.clock_dial_color, CIRCLE_SHAPE)
            .on_size_changed(move |size: compose_ui::IntSize| {
                if let Some(inner) = state.inner() {
                    inner
                        .dial_center
                        .set(Offset::new((size.width / 2) as f32, (size.height / 2) as f32));
                    inner.current_diameter.set(Dp(size.width as f32 / size_density.density));
                }
            })
            .on_globally_positioned(move |coordinates: compose_ui::LayoutCoordinates| {
                if let Some(inner) = state.inner() {
                    inner.dial_position.set(coordinates.position_in_root());
                }
            })
            .pointer_input((state, auto_switch_to_minute), move |scope| {
                let (press_focus, tap_focus) = (press_focus.clone(), tap_focus.clone());
                let (tap_spec, tap_distance) = (tap_spec.clone(), tap_distance.clone());
                async move {
                    scope
                        .detect_tap_gestures(
                            TapGestures::default()
                                .on_press(move |at: Offset| {
                                    press_focus.clear_focus(false);
                                    if let Some(inner) = state.inner() {
                                        inner.press.set(at);
                                    }
                                })
                                .on_tap(move |at: Offset| {
                                    tap_focus.clear_focus(false);
                                    let (spec, distance) = (tap_spec.clone(), tap_distance());
                                    coroutine_scope.launch(async move {
                                        let center = state
                                            .inner()
                                            .map_or(Offset::ZERO, |inner| inner.dial_center.get());
                                        state
                                            .on_tap(
                                                at.x,
                                                at.y,
                                                distance,
                                                auto_switch_to_minute,
                                                center,
                                                spec,
                                            )
                                            .await;
                                    });
                                }),
                        )
                        .await;
                }
            })
            .pointer_input((state, auto_switch_to_minute, 1), move |scope| {
                let (drag_spec, drag_distance) = (drag_spec.clone(), drag_distance.clone());
                async move {
                    let end_spec = drag_spec.clone();
                    scope
                        .detect_drag_gestures(
                            DragGestures::default()
                                .on_drag_end(move || {
                                    let spec = end_spec.clone();
                                    coroutine_scope.launch(async move {
                                        if auto_switch_to_minute {
                                            state.set_selection(TimePickerSelectionMode::Minute);
                                        }
                                        state.on_gesture_end(spec).await;
                                    });
                                })
                                .on_drag(move |_, amount: Offset| {
                                    let (spec, distance) = (drag_spec.clone(), drag_distance());
                                    coroutine_scope.launch(async move {
                                        let Some(inner) = state.inner() else { return };
                                        let at = inner.press.get() + amount;
                                        inner.press.set(at);
                                        let center = inner.dial_center.get();
                                        state
                                            .rotate_to(
                                                atan(at.y - center.y, at.x - center.x),
                                                spec,
                                                false,
                                            )
                                            .await;
                                        state.move_selector(at.x, at.y, distance, center);
                                    });
                                }),
                        )
                        .await;
                }
            })
            .then(draw_selector(state, colors))
            .focus_group(),
        animation_spec = MaterialTheme::motion_scheme().default_effects_spec(),
        |screen: TimePickerSelectionMode| {
            let is_active_screen = screen == state.selection();
            let is_minute = state.selection() == TimePickerSelectionMode::Minute;
            let twenty_four = state.is_24_hour() && !is_minute;
            let values = if screen == TimePickerSelectionMode::Minute { MINUTES } else { HOURS };
            CircularLayout!(
                Modifier.size(CLOCK_DIAL_CONTAINER_SIZE).selectable_group(),
                OUTER_CIRCLE_TO_SIZE_RATIO,
                {
                    CompositionLocalProvider(
                        LOCAL_CONTENT_COLOR.provides(colors.clock_dial_content_color),
                        || {
                            for (index, value) in values.iter().enumerate() {
                                let outer_value = if !state.is_24_hour() || is_minute {
                                    *value
                                } else {
                                    *value % 12
                                };
                                let first = index == 0;
                                let last = index == values.len() - 1;
                                ClockText!(
                                    dial_item_modifier(
                                        index,
                                        first.then_some(first_outer),
                                        last.then_some(last_outer),
                                        first.then_some(if twenty_four {
                                            last_inner
                                        } else {
                                            last_outer
                                        }),
                                        last.then_some(if twenty_four {
                                            first_inner
                                        } else {
                                            first_outer
                                        }),
                                    ),
                                    state,
                                    outer_value,
                                    auto_switch_to_minute,
                                    is_active_screen,
                                    colors
                                );
                            }
                            if twenty_four {
                                CircularLayout!(
                                    Modifier
                                        .layout_id(LayoutId::InnerCircle)
                                        .size(CLOCK_DIAL_CONTAINER_SIZE),
                                    INNER_CIRCLE_TO_SIZE_RATIO,
                                    {
                                        for (index, value) in HOURS.iter().enumerate() {
                                            let first = index == 0;
                                            let last = index == HOURS.len() - 1;
                                            ClockText!(
                                                dial_item_modifier(
                                                    index + 12,
                                                    first.then_some(first_inner),
                                                    last.then_some(last_inner),
                                                    first.then_some(last_outer),
                                                    last.then_some(first_outer),
                                                ),
                                                state,
                                                *value % 12 + 12,
                                                auto_switch_to_minute,
                                                is_active_screen,
                                                colors
                                            );
                                        }
                                    }
                                );
                            }
                        },
                    );
                }
            );
        }
    );
}

/// A number's place in the order of the dial and the loop of its focus.
fn dial_item_modifier(
    index: usize,
    first_requester: Option<FocusRequester>,
    last_requester: Option<FocusRequester>,
    previous: Option<FocusRequester>,
    next: Option<FocusRequester>,
) -> Modifier {
    let mut modifier =
        Modifier.semantics(move |config| config.set_traversal_index(index as f32 + 1.0));
    if let Some(requester) = first_requester {
        modifier = modifier.focus_requester(requester);
    }
    if let Some(requester) = last_requester {
        modifier = modifier.focus_requester(requester);
    }
    modifier.focus_properties(move |properties| {
        if let Some(previous) = previous {
            properties.previous = previous;
        }
        if let Some(next) = next {
            properties.next = next;
        }
    })
}

/// Draws the selector behind the numbers, as Compose's `drawSelector` without its blending.
fn draw_selector(state: TimePickerState, colors: TimePickerColors) -> Modifier {
    Modifier.draw_behind(move |scope| {
        let Some(inner) = state.inner() else { return };
        let density = scope.density();
        let position = state.selector_position();
        let handle = Offset::new(density.to_px(Dp(position.x)), density.to_px(Dp(position.y)));
        let diameter = density.round_to_px(inner.current_diameter.get()) as f32;
        let radius = density.to_px(SELECTOR_HANDLE_SIZE) / 2.0 * diameter
            / density.round_to_px(CLOCK_DIAL_CONTAINER_SIZE) as f32;
        let color = colors.selector_color;
        scope.draw_circle(handle, radius, Paint::fill(color));
        let angle = inner.angle.value();
        let end = Offset::new(handle.x - radius * angle.cos(), handle.y - radius * angle.sin());
        let center = scope.center();
        scope.draw_line(center, end, Paint::stroke(color, density.to_px(SELECTOR_TRACK_WIDTH)));
        scope.draw_circle(center, density.to_px(SELECTOR_CENTER_SIZE) / 2.0, Paint::fill(color));
    })
}

/// As Compose's `ClockText`.
#[composable]
fn ClockText(
    modifier: Modifier,
    state: TimePickerState,
    value: i32,
    auto_switch_to_minute: bool,
    is_active_screen: bool,
    colors: TimePickerColors,
) {
    let Some(inner) = state.inner() else { return };
    let input_mode = LOCAL_INPUT_MODE_MANAGER.current();
    let focus_manager = LOCAL_FOCUS_MANAGER.current();
    let density = LOCAL_DENSITY.current();
    let max_distance = density.to_px(MAX_DISTANCE);
    // Its bounds in the root: the dial is where the state says.
    let bounds = remember(|| mutable_state_of(Rect::new(0.0, 0.0, 0.0, 0.0)));
    let interaction_source = remember(MutableInteractionSource::new);
    let coroutine_scope = remember_coroutine_scope();
    let description = number_content_description(state.selection(), state.is_24_hour(), value);
    let text = to_local_string(value, 1, 40, &default_locale());
    let is_the_selected_value = match state.selection() {
        TimePickerSelectionMode::Hour if state.is_24_hour() => value == state.hour(),
        TimePickerSelectionMode::Hour => value == state.hour_for_display(),
        TimePickerSelectionMode::Minute => {
            ((state.minute() as f32 / 5.0).round() as i32 * 5) % 60 == value
        }
    };
    let selected = remember_is_under_selector(state, density, bounds);
    let dial = inner.dial_focus_requester;
    if is_the_selected_value && is_active_screen {
        let focusable = state.is_dial_focusable();
        LaunchedEffect(focusable, async move {
            if focusable {
                dial.request_focus();
            }
        });
    }
    let on_click = move |auto_switch: bool| {
        coroutine_scope.launch(async move {
            let Some(inner) = state.inner() else { return };
            let origin = inner.dial_position.get();
            let bounds = bounds.get();
            let center = Offset::new(
                (bounds.left + bounds.right) / 2.0 - origin.x,
                (bounds.top + bounds.bottom) / 2.0 - origin.y,
            );
            let dial_center = inner.dial_center.get();
            state
                .on_tap(center.x, center.y, max_distance, auto_switch, dial_center, snap(0).into())
                .await;
        });
    };
    let hour_node = inner.hour_node_focus_requester;
    let minute_node = inner.minute_node_focus_requester;
    let am_pm_node = inner.am_pm_node_focus_requester;
    let key_focus = focus_manager.clone();
    let on_key = move |event: KeyEvent| {
        if event.kind == KeyEventType::KeyDown {
            match event.key {
                Key::DirectionDown | Key::DirectionRight => {
                    key_focus.move_focus(FocusDirection::Next);
                    return true;
                }
                Key::DirectionUp | Key::DirectionLeft => {
                    key_focus.move_focus(FocusDirection::Previous);
                    return true;
                }
                Key::Tab => {
                    let hours = state.selection() == TimePickerSelectionMode::Hour;
                    if event.modifiers.shift {
                        state.set_is_dial_focusable(false);
                        if hours {
                            hour_node.request_focus();
                        } else {
                            minute_node.request_focus();
                        }
                    } else if hours {
                        minute_node.request_focus();
                    } else if state.is_24_hour() {
                        minute_node.request_focus();
                        key_focus.move_focus(FocusDirection::Next);
                    } else {
                        am_pm_node.request_focus();
                    }
                    return true;
                }
                _ => {}
            }
        }
        if event.kind == KeyEventType::KeyUp
            && matches!(
                event.key,
                Key::DirectionCenter | Key::Enter | Key::NumPadEnter | Key::Space
            )
        {
            on_click(false);
            return true;
        }
        false
    };
    let requester_modifier =
        if is_the_selected_value { Modifier.focus_requester(dial) } else { Modifier };
    let is_selected = selected.get();
    let style = MaterialTheme::typography().body_large.clone();
    Box!(
        modifier = modifier
            .then(requester_modifier)
            .on_globally_positioned(move |coordinates: compose_ui::LayoutCoordinates| {
                let at = coordinates.position_in_root();
                let size = coordinates.size();
                bounds.set(Rect::new(
                    at.x,
                    at.y,
                    at.x + size.width as f32,
                    at.y + size.height as f32,
                ));
            })
            .then(minimum_interactive_component_size(Modifier))
            .size(MINIMUM_INTERACTIVE_SIZE)
            .on_key_event(on_key)
            .focus_properties(move |properties| {
                properties.can_focus = is_active_screen
                    && state.is_dial_focusable()
                    && input_mode.input_mode() != InputMode::Touch;
            })
            .indication(
                interaction_source.clone(),
                ripple(true, Dp(MINIMUM_INTERACTIVE_SIZE.0 / 2.0), Color::UNSPECIFIED)
            )
            .focusable_with(true, Some(interaction_source))
            .semantics_with(true, move |config| {
                config.on_click(None, move || {
                    on_click(auto_switch_to_minute);
                    true
                });
                config.set_selected(is_selected);
            }),
        content_alignment = Alignment::CENTER,
        {
            let description = description.clone();
            Text!(
                "{text}",
                modifier = Modifier.clear_and_set_semantics(move |config| {
                    config.set_content_description(description.clone());
                }),
                color = if is_selected {
                    colors.clock_dial_selected_content_color
                } else {
                    colors.clock_dial_content_color
                },
                style = style.clone()
            );
        }
    );
}

/// Whether the selector's handle is on a number, as `ClockText`'s `derivedStateOf`.
fn remember_is_under_selector(
    state: TimePickerState,
    density: compose_ui::Density,
    bounds: MutableState<Rect>,
) -> compose_runtime::State<bool> {
    compose_runtime::remember_keyed((state, density), move || {
        derived_state_of(move || {
            let Some(inner) = state.inner() else { return false };
            let origin = inner.dial_position.get();
            let position = state.selector_position();
            let x = density.to_px(Dp(position.x)) + origin.x;
            let y = density.to_px(Dp(position.y)) + origin.y;
            let bounds = bounds.get();
            x >= bounds.left && x < bounds.right && y >= bounds.top && y < bounds.bottom
        })
    })
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum LayoutId {
    InnerCircle,
}

/// As Compose's `CircularLayout`: its children evenly on a circle, from the top.
#[composable]
fn CircularLayout(modifier: Modifier, radius_to_size_ratio: f32, content: impl Fn() + 'static) {
    let policy = remember(move || {
        measure_policy(move |scope, measurables: &[Measurable], constraints: Constraints| {
            let radius = constraints.max_height as f32 * radius_to_size_ratio;
            let item_constraints = Constraints { min_width: 0, min_height: 0, ..constraints };
            let (inner, items): (Vec<Measurable>, Vec<Measurable>) =
                measurables.iter().copied().partition(|m| scope.layout_id(*m).is_some());
            let theta = FULL_CIRCLE / items.len().max(1) as f32;
            for (index, &item) in items.iter().enumerate() {
                let item = scope.measure(item, item_constraints);
                let center_x = constraints.max_width / 2 - item.width / 2;
                let center_y = constraints.max_height / 2 - item.height / 2;
                let angle = theta * index as f32 - QUARTER_CIRCLE;
                let x = radius * angle.cos() + center_x as f32;
                let y = radius * angle.sin() + center_y as f32;
                scope.place(&item, x.round() as i32, y.round() as i32);
            }
            for &circle in &inner {
                let circle = scope.measure(circle, item_constraints);
                scope.place(
                    &circle,
                    (constraints.min_width - circle.width) / 2,
                    (constraints.min_height - circle.height) / 2,
                );
            }
            scope.layout(constraints.min_width, constraints.min_height)
        })
    });
    compose_ui::Layout(modifier, policy, compose_runtime::callback(move || content()));
}

/// A time input, as Compose's `TimeInput`: a field for the hour, one for the minute, and the
/// period.
#[composable]
pub fn TimeInput(
    state: TimePickerState,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::TimeInputDefaults::colors())] colors: TimeInputColors,
) {
    let locale = default_locale();
    let hour_text_value = {
        let locale = locale.clone();
        move || {
            let value = if state.is_hour_input_valid() {
                state.hour_for_display()
            } else {
                state.hour_input()
            };
            to_local_string(value, 2, 40, &locale)
        }
    };
    let minute_text_value = {
        let locale = locale.clone();
        move || {
            let value =
                if state.is_minute_input_valid() { state.minute() } else { state.minute_input() };
            to_local_string(value, 2, 40, &locale)
        }
    };
    let hour_text = {
        let value = hour_text_value.clone();
        remember_saveable(move || mutable_state_of(value()))
    };
    let minute_text = {
        let value = minute_text_value.clone();
        remember_saveable(move || mutable_state_of(value()))
    };
    let hour_override = remember(|| std::rc::Rc::new(Cell::new(false)));
    let minute_override = remember(|| std::rc::Rc::new(Cell::new(false)));
    {
        let hour_override = hour_override.clone();
        LaunchedEffect(state.hour(), async move {
            if hour_override.get() {
                let text = hour_text_value();
                if hour_text.get() != text {
                    hour_text.set(text);
                }
            }
            hour_override.set(true);
        });
    }
    {
        let minute_override = minute_override.clone();
        LaunchedEffect(state.minute(), async move {
            if minute_override.get() {
                let text = minute_text_value();
                if minute_text.get() != text {
                    minute_text.set(text);
                }
            }
            minute_override.set(true);
        });
    }
    let style = MaterialTheme::typography().display_medium.clone();
    let style = TextStyle {
        text_align: Some(TextAlign::Center),
        color: colors.time_text_field_colors.focused_text_color,
        ..style
    };
    let haptics = LOCAL_HAPTIC_FEEDBACK.current();
    let on_error = compose_runtime::callback(move || {
        haptics.perform_haptic_feedback(HapticFeedbackType::Reject);
    });
    Row!(
        modifier = modifier.clone().semantics(|config| config.set_is_traversal_group(true)),
        vertical_alignment = Alignment::TOP,
        {
            CompositionLocalProvider(
                (
                    LOCAL_TEXT_STYLE.provides(style.clone()),
                    LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Ltr),
                ),
                || {
                    Row!({
                        TimePickerTextField!(
                            Modifier,
                            hour_text,
                            hour_override.clone(),
                            state,
                            TimePickerSelectionMode::Hour,
                            colors,
                            on_error.clone()
                        );
                        DisplaySeparator!(
                            Modifier.size((DISPLAY_SEPARATOR_WIDTH, INPUT_PERIOD_SELECTOR_HEIGHT))
                        );
                        TimePickerTextField!(
                            Modifier,
                            minute_text,
                            minute_override.clone(),
                            state,
                            TimePickerSelectionMode::Minute,
                            colors,
                            on_error.clone()
                        );
                    });
                },
            );
            if !state.is_24_hour() {
                Box!(
                    modifier =
                        Modifier.padding((PERIOD_TOGGLE_PADDING_SMALL, Dp(0.0), Dp(0.0), Dp(0.0))),
                    {
                        PeriodToggle!(
                            Modifier
                                .size((INPUT_PERIOD_SELECTOR_WIDTH, INPUT_PERIOD_SELECTOR_HEIGHT)),
                            state,
                            colors,
                            false
                        );
                    }
                );
            }
        }
    );
}

/// What `TimeInputTransformation` leaves of what's been typed: `None` is nothing. A field of a
/// `String` tells no cursor: the digit a third one replaces both by is the one that isn't in the
/// old text, wherever it went (D1566).
fn transform_input(
    new_text: &str,
    old_text: &str,
    selection: TimePickerSelectionMode,
    state: TimePickerState,
    user_override: &Cell<bool>,
    locale: &Locale,
    on_error: &dyn Fn(),
) -> Option<String> {
    let hours = selection == TimePickerSelectionMode::Hour;
    if new_text.is_empty() {
        let target = if !hours {
            0
        } else if state.is_pm() && !state.is_24_hour() {
            12
        } else {
            0
        };
        if (if hours { state.hour() } else { state.minute() }) != target {
            user_override.set(false);
        }
        if hours {
            state.set_hour_input(target);
        } else {
            state.set_minute_input(target);
        }
        return Some(String::new());
    }
    if !new_text.chars().all(|character| is_date_digit(character, locale)) {
        return None;
    }
    let new_chars: Vec<char> = new_text.chars().collect();
    let old_chars: Vec<char> = old_text.chars().collect();
    let has_inserted_text = !is_subsequence(&new_chars, &old_chars);
    let mut kept = new_chars.clone();
    if old_chars.len() == 2 && new_chars.len() == 3 {
        let at = (0..2).find(|&index| new_chars[index] != old_chars[index]).unwrap_or(2);
        kept = vec![new_chars[at]];
    }
    if kept.len() > 2 {
        on_error();
        return None;
    }
    let new_value =
        kept.iter().fold(0, |value, character| value * 10 + digit_value(*character, locale));
    if hours {
        let target = if state.is_24_hour() {
            new_value
        } else if new_value == 12 {
            if state.is_pm() { 12 } else { 0 }
        } else if state.is_pm() {
            new_value + 12
        } else {
            new_value
        };
        let inner = state.inner()?;
        if inner.is_valid_hour(target) && state.hour() != target {
            user_override.set(false);
        }
        state.set_hour_input(target);
        let twelve_hour_auto_advance = !state.is_24_hour() && (2..=9).contains(&new_value);
        if (kept.len() == 2 || (has_inserted_text && twelve_hour_auto_advance))
            && state.is_hour_input_valid()
        {
            state.set_selection(TimePickerSelectionMode::Minute);
        }
    } else {
        if (0..=MAX_MINUTE_VALUE).contains(&new_value) && state.minute() != new_value {
            user_override.set(false);
        }
        state.set_minute_input(new_value);
    }
    Some(kept.into_iter().collect())
}

/// Whether `shorter` is what's left of `longer` by taking characters away.
fn is_subsequence(shorter: &[char], longer: &[char]) -> bool {
    let mut rest = longer.iter();
    shorter.iter().all(|character| rest.any(|other| other == character))
}

/// As Compose's `TimePickerTextField`: unselected, a selector over a field that stays there
/// without a size, to take the focus.
#[composable]
#[allow(clippy::too_many_arguments)]
fn TimePickerTextField(
    modifier: Modifier,
    text: MutableState<String>,
    user_override: std::rc::Rc<Cell<bool>>,
    state: TimePickerState,
    selection: TimePickerSelectionMode,
    colors: TimeInputColors,
    on_error: impl Fn() + 'static,
) {
    let focus_requester = remember(FocusRequester::new);
    let text_field_colors = colors.time_text_field_colors;
    let selected = selection == state.selection();
    let hours = selection == TimePickerSelectionMode::Hour;
    let is_valid = if hours { state.is_hour_input_valid() } else { state.is_minute_input_valid() };
    let text_color = if is_valid {
        colors.time_selector_content_color_of(selected)
    } else {
        MaterialTheme::color_scheme().error
    };
    let size = (TIME_FIELD_WIDTH, TIME_FIELD_HEIGHT);
    let locale = default_locale();
    let interaction_source = remember(MutableInteractionSource::new);
    Column!(modifier = modifier.clone().width(IntrinsicSize::Min), {
        if !selected {
            let value = match selection {
                TimePickerSelectionMode::Hour if state.is_hour_input_valid() => {
                    state.hour_for_display()
                }
                TimePickerSelectionMode::Hour => state.hour_input(),
                TimePickerSelectionMode::Minute => state.minute_input(),
            };
            TimeSelector!(Modifier.size(size), value, state, selection, colors, is_valid, || {});
        }
        let policy = remember_visible_policy(selected);
        let user_override = user_override.clone();
        let on_error = on_error.clone();
        let locale = locale.clone();
        compose_ui::Layout(
            Modifier.clip_to_bounds(),
            policy,
            compose_runtime::callback(move || {
                let user_override = user_override.clone();
                let on_error = on_error.clone();
                let locale = locale.clone();
                let current = text.get();
                let decoration_text = current.clone();
                BasicTextField!(
                    current,
                    move |new_text: String| {
                        let old_text = text.get();
                        if let Some(kept) = transform_input(
                            &new_text,
                            &old_text,
                            selection,
                            state,
                            &user_override,
                            &locale,
                            &*on_error,
                        ) {
                            text.set(kept);
                        }
                    },
                    modifier = Modifier.focus_requester(focus_requester).size(size).semantics(
                        move |config| {
                            config.set_content_description(if hours {
                                "for hour"
                            } else {
                                "for minutes"
                            });
                            config.set_max_text_length(2);
                        }
                    ),
                    text_style = TextStyle { color: text_color, ..LOCAL_TEXT_STYLE.current() },
                    keyboard_options = KeyboardOptions {
                        ime_action: if hours { ImeAction::Next } else { ImeAction::Done },
                        keyboard_type: KeyboardType::Number,
                        ..KeyboardOptions::DEFAULT
                    },
                    keyboard_actions = if hours {
                        KeyboardActions {
                            on_next: Some(compose_runtime::callback(move || {
                                if state.is_hour_input_valid() {
                                    state.set_selection(TimePickerSelectionMode::Minute);
                                }
                            })),
                            ..KeyboardActions::new()
                        }
                    } else {
                        KeyboardActions::new()
                    },
                    single_line = true,
                    cursor_color = MaterialTheme::color_scheme().primary,
                    interaction_source = Some(interaction_source),
                    decoration_box = move |inner_text_field: compose_runtime::Callback| {
                        OutlinedTextFieldDecorationBox!(
                            decoration_text.clone(),
                            inner_text_field,
                            true,
                            true,
                            VisualTransformation::NONE,
                            interaction_source,
                            is_error = !is_valid,
                            colors = text_field_colors,
                            content_padding = Some(PaddingValues::from(Dp(0.0))),
                            container = Some(move || {
                                OutlinedTextFieldContainer!(
                                    true,
                                    !is_valid,
                                    interaction_source,
                                    colors = text_field_colors,
                                    shape = MaterialTheme::shapes().small.clone(),
                                    focused_border_thickness = Dp(2.0),
                                    unfocused_border_thickness = Dp(1.0)
                                );
                            })
                        );
                    }
                );
            }),
        );
        SupportingText!(Modifier.fill_max_width(), selection, state, is_valid);
    });
    let selected_now = state.selection();
    LaunchedEffect(selected_now, async move {
        if selected_now == selection {
            focus_requester.request_focus();
        }
    });
}

/// A layout of the size of its child, or of none, as Compose's `Modifier.visible`: here the child
/// is placed all the same, clipped away, as a node that isn't placed isn't in the semantics tree.
fn remember_visible_policy(visible: bool) -> std::rc::Rc<dyn MeasurePolicy> {
    compose_runtime::remember_keyed(visible, move || {
        measure_policy(move |scope, measurables: &[Measurable], constraints: Constraints| {
            let child = scope.measure(measurables[0], constraints);
            scope.place(&child, 0, 0);
            if visible { scope.layout(child.width, child.height) } else { scope.layout(0, 0) }
        })
    })
}

/// As Compose's `SupportingText`.
#[composable]
fn SupportingText(
    modifier: Modifier,
    selection: TimePickerSelectionMode,
    state: TimePickerState,
    is_valid: bool,
) {
    let hours = selection == TimePickerSelectionMode::Hour;
    let text = match () {
        _ if is_valid && hours => "Hour",
        _ if is_valid => "Minute",
        _ if hours && state.is_24_hour() => "Hour must be 0–23",
        _ if hours => "Hour must be 1–12",
        _ => "Minute must be 0–59",
    };
    let scheme = MaterialTheme::color_scheme();
    let padded = modifier.padding((Dp(0.0), SUPPORT_LABEL_TOP, Dp(0.0), Dp(0.0)));
    Text!(
        "{text}",
        modifier = if is_valid {
            padded.clear_and_set_semantics(|_| {})
        } else {
            padded.semantics(|config| config.set_live_region(LiveRegionMode::Polite))
        },
        color = if is_valid { scheme.on_surface_variant } else { scheme.error },
        min_lines = 2,
        style = MaterialTheme::typography().body_small.clone()
    );
}
