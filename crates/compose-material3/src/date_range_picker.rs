//! The date range picker, as Compose's `DateRangePicker` and `DateRangeInput`:
//! `DateRangePickerState`, `DateRangePickerDefaults`, the vertical list of months and the range's
//! background. Their months, days and fields are `date_picker.rs`'s and `date_input.rs`'s, with
//! the limits written there (D1562): the locale tables, English strings, a field of a `String`.

use std::ops::RangeInclusive;
use std::rc::Rc;

use compose_animation::Crossfade;
use compose_foundation::gestures::ScrollableState as _;
use compose_foundation::layout::{Arrangement, Column, Row};
use compose_foundation::{LazyColumn, LazyItemModifiers, LazyListState, remember_lazy_list_state};
use compose_runtime::{
    Callback, ComposeArg, CompositionLocal, CompositionLocalProvider, CoroutineScope,
    LaunchedEffect, ObjectHandle, Owned, OwnedState, composable, mutable_state_of, remember,
    remember_coroutine_scope, remember_keyed,
};
use compose_ui::{
    Alignment, Color, CustomAccessibilityAction, Dp, DrawScope, FocusDirection, FocusRequester,
    KeyEvent, LOCAL_FOCUS_MANAGER, LayoutDirection, LiveRegionMode, Locale, Modifier, Paint, Rect,
    ScrollAxisRange,
};

use crate::calendar_model::{
    CalendarDate, CalendarModel, CalendarMonth, DAYS_IN_WEEK, default_locale,
};
use crate::components::Text;
use crate::date_input::{DateInputTextField, DateInputValidator, InputIdentifier};
use crate::date_picker::{
    BaseDatePickerState, DATE_PICKER_HORIZONTAL_PADDING, DateEntryContainer, DatePickerColors,
    DatePickerDefaults, DatePickerFormatter, DisplayMode, DisplayModeToggleButton, Month,
    RECOMMENDED_SIZE_FOR_ACCESSIBILITY, SelectableDates, WeekDays, YearRange, is_shift_tab, is_tab,
    number_of_months_in_range, update_displayed_month,
};
use crate::theme::{MaterialTheme, ProvideTextStyle};

struct DateRangePickerStateInner {
    base: BaseDatePickerState,
    selected_start_date: OwnedState<Option<CalendarDate>>,
    selected_end_date: OwnedState<Option<CalendarDate>>,
    display_mode: OwnedState<DisplayMode>,
}

/// A date range picker's state, as Compose's `DateRangePickerState`: make one with
/// [`DateRangePickerState::new`], or use [`remember_date_range_picker_state`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DateRangePickerState {
    object: ObjectHandle<DateRangePickerStateInner>,
}

impl DateRangePickerStateInner {
    /// As `setSelection`: a start out of the years, a start after the end, or an end without a
    /// start selects nothing, where a day out of the years alone is none.
    fn set_selection(&self, start_date_millis: Option<i64>, end_date_millis: Option<i64>) {
        let date = |millis: Option<i64>| {
            millis
                .map(|millis| self.base.calendar_model.get_canonical_date(millis))
                .filter(|date| self.base.year_range.contains(&date.year))
        };
        let (start, end) = (date(start_date_millis), date(end_date_millis));
        let valid = start.is_some_and(|start| end.is_none_or(|end| start <= end));
        self.selected_start_date.set(if valid { start } else { None });
        self.selected_end_date.set(if valid { end } else { None });
    }
}

impl DateRangePickerState {
    /// Its object, or `None` once its owner went (D240): then it does nothing.
    #[track_caller]
    fn inner(&self) -> Option<Rc<DateRangePickerStateInner>> {
        self.object.get()
    }

    /// As `DateRangePickerState(locale, …)`: the dates as [`DateRangePickerState::set_selection`]
    /// takes them; no displayed month is the start date's month, and a month out of `year_range`,
    /// or none at all, the current one.
    #[track_caller]
    pub fn new(
        locale: Locale,
        initial_selected_start_date_millis: Option<i64>,
        initial_selected_end_date_millis: Option<i64>,
        initial_displayed_month_millis: Option<i64>,
        year_range: RangeInclusive<i32>,
        initial_display_mode: DisplayMode,
        selectable_dates: Rc<dyn SelectableDates>,
    ) -> Owned<DateRangePickerState> {
        let inner = DateRangePickerStateInner {
            base: BaseDatePickerState::new(
                locale,
                initial_displayed_month_millis.or(initial_selected_start_date_millis),
                year_range,
                selectable_dates,
            ),
            selected_start_date: mutable_state_of(None),
            selected_end_date: mutable_state_of(None),
            display_mode: mutable_state_of(initial_display_mode),
        };
        inner.set_selection(initial_selected_start_date_millis, initial_selected_end_date_millis);
        Owned::new(inner, |object| DateRangePickerState { object })
    }

    /// The start day's first millisecond, as `selectedStartDateMillis`.
    pub fn selected_start_date_millis(&self) -> Option<i64> {
        self.inner()?.selected_start_date.with(|date| date.map(|date| date.utc_time_millis))
    }

    /// The end day's first millisecond, as `selectedEndDateMillis`.
    pub fn selected_end_date_millis(&self) -> Option<i64> {
        self.inner()?.selected_end_date.with(|date| date.map(|date| date.utc_time_millis))
    }

    /// As `setSelection`: each date is rounded down to its day. A day out of the years is none; a
    /// start that's none, or after the end, selects nothing: Compose's documentation says it
    /// throws, and its code does this.
    pub fn set_selection(&self, start_date_millis: Option<i64>, end_date_millis: Option<i64>) {
        if let Some(inner) = self.inner() {
            inner.set_selection(start_date_millis, end_date_millis);
        }
    }

    /// The displayed month's first millisecond, as `displayedMonthMillis`.
    pub fn displayed_month_millis(&self) -> i64 {
        self.inner().map_or(0, |inner| inner.base.displayed_month_millis())
    }

    /// As setting `displayedMonthMillis`: a month out of the years is ignored.
    pub fn set_displayed_month_millis(&self, month_millis: i64) {
        if let Some(inner) = self.inner() {
            inner.base.set_displayed_month_millis(month_millis);
        }
    }

    /// As `displayMode`.
    pub fn display_mode(&self) -> DisplayMode {
        self.inner().map_or(DisplayMode::Picker, |inner| inner.display_mode.with(|mode| *mode))
    }

    /// As setting `displayMode`: the displayed month goes to the start date's.
    pub fn set_display_mode(&self, display_mode: DisplayMode) {
        if let Some(inner) = self.inner() {
            if let Some(start) = self.selected_start_date_millis() {
                inner.base.set_displayed_month_millis(
                    inner.base.calendar_model.get_month_of_millis(start).start_utc_time_millis,
                );
            }
            inner.display_mode.set(display_mode);
        }
    }

    /// As `yearRange`.
    pub fn year_range(&self) -> RangeInclusive<i32> {
        self.inner().map_or(DatePickerDefaults::YEAR_RANGE, |inner| inner.base.year_range.clone())
    }

    /// As `selectableDates`.
    pub fn selectable_dates(&self) -> Rc<dyn SelectableDates> {
        self.inner().map_or_else(DatePickerDefaults::all_dates, |inner| {
            inner.base.selectable_dates.with(|dates| dates.0.clone())
        })
    }

    fn set_selectable_dates(&self, selectable_dates: Rc<dyn SelectableDates>) {
        if let Some(inner) = self.inner() {
            inner
                .base
                .selectable_dates
                .set(crate::date_picker::SelectableDatesRef(selectable_dates));
        }
    }

    /// As `locale`.
    pub fn locale(&self) -> Locale {
        self.inner().map_or_else(
            || Locale::new("en-US"),
            |inner| inner.base.calendar_model.locale().clone(),
        )
    }

    fn calendar_model(&self) -> CalendarModel {
        self.inner().map_or_else(
            || CalendarModel::new(Locale::new("en-US")),
            |inner| inner.base.calendar_model.clone(),
        )
    }

    /// What's saved of a state, as `DateRangePickerStateImpl.Saver`: the selectable dates and the
    /// locale come back from the caller.
    fn saver(
        selectable_dates: Rc<dyn SelectableDates>,
        locale: Locale,
    ) -> impl compose_runtime::Saver<Owned<DateRangePickerState>> {
        use compose_runtime::Saveable;
        compose_runtime::saver(
            |state: &Owned<DateRangePickerState>| {
                state.inner()?;
                let range = state.year_range();
                let mode = i32::from(state.display_mode() == DisplayMode::Input);
                Some(
                    (
                        (state.selected_start_date_millis(), state.selected_end_date_millis()),
                        state.displayed_month_millis(),
                        (*range.start(), *range.end()),
                        mode,
                    )
                        .save(),
                )
            },
            move |saved| {
                <((Option<i64>, Option<i64>), i64, (i32, i32), i32)>::restore(saved).map(
                    |((start, end), displayed, (first, last), mode)| {
                        DateRangePickerState::new(
                            locale.clone(),
                            start,
                            end,
                            Some(displayed),
                            first..=last,
                            if mode == 1 { DisplayMode::Input } else { DisplayMode::Picker },
                            selectable_dates.clone(),
                        )
                    },
                )
            },
        )
    }
}

impl ComposeArg for DateRangePickerState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A date range picker state remembered here and saved, in the default locale, as
/// `rememberDateRangePickerState`. `initial_displayed_month_millis` left out is the start date's
/// month, as Compose's default, or else the current one (D1562).
#[composable(non_restartable)]
pub fn remember_date_range_picker_state(
    #[default(None)] initial_selected_start_date_millis: Option<i64>,
    #[default(None)] initial_selected_end_date_millis: Option<i64>,
    #[default(None)] initial_displayed_month_millis: Option<i64>,
    #[default(crate::DatePickerDefaults::YEAR_RANGE)] year_range: RangeInclusive<i32>,
    #[default(crate::DisplayMode::Picker)] initial_display_mode: DisplayMode,
    #[default(crate::DatePickerDefaults::all_dates())] selectable_dates: Rc<dyn SelectableDates>,
) -> DateRangePickerState {
    let locale = default_locale();
    let saver = DateRangePickerState::saver(selectable_dates.clone(), locale.clone());
    let initial_selectable_dates = selectable_dates.clone();
    let state = compose_runtime::remember_saveable_with(saver, move || {
        DateRangePickerState::new(
            locale.clone(),
            initial_selected_start_date_millis,
            initial_selected_end_date_millis,
            initial_displayed_month_millis,
            year_range.clone(),
            initial_display_mode,
            initial_selectable_dates.clone(),
        )
    });
    state.set_selectable_dates(selectable_dates);
    state
}

/// A date range picker's defaults, as Compose's `DateRangePickerDefaults`.
pub struct DateRangePickerDefaults;

impl DateRangePickerDefaults {
    /// The title, by the mode, as `DateRangePickerTitle`.
    pub fn date_range_picker_title(
        display_mode: DisplayMode,
        modifier: Modifier,
        content_color: Color,
    ) {
        let text = match display_mode {
            DisplayMode::Picker => "Select dates",
            DisplayMode::Input => "Enter dates",
        };
        Text!("{text}", modifier = modifier, color = content_color);
    }

    /// The headline, each date or what says there's none, as `DateRangePickerHeadline`.
    pub fn date_range_picker_headline(
        selected_start_date_millis: Option<i64>,
        selected_end_date_millis: Option<i64>,
        display_mode: DisplayMode,
        date_formatter: &dyn DatePickerFormatter,
        modifier: Modifier,
        content_color: Color,
    ) {
        const START_DATE_TEXT: &str = "Start date";
        const END_DATE_TEXT: &str = "End date";
        let locale = default_locale();
        // Compose's two strings for no date, by the mode, say the same in English.
        let no_date = match display_mode {
            DisplayMode::Picker | DisplayMode::Input => "None",
        };
        let verbose = |millis| {
            date_formatter.format_date(millis, &locale, true).unwrap_or_else(|| no_date.to_string())
        };
        let description = format!(
            "{START_DATE_TEXT}: {}, {END_DATE_TEXT}: {}",
            verbose(selected_start_date_millis),
            verbose(selected_end_date_millis),
        );
        let start = date_formatter
            .format_date(selected_start_date_millis, &locale, false)
            .unwrap_or_else(|| START_DATE_TEXT.to_string());
        let end = date_formatter
            .format_date(selected_end_date_millis, &locale, false)
            .unwrap_or_else(|| END_DATE_TEXT.to_string());
        Row!(
            modifier = modifier.clear_and_set_semantics(move |config| {
                config.set_live_region(LiveRegionMode::Polite);
                config.set_content_description(description.clone());
            }),
            horizontal_arrangement = Arrangement::spaced_by(Dp(4.0)),
            vertical_alignment = Alignment::CENTER_VERTICALLY,
            {
                Text!("{start}", color = content_color);
                Text!("-", color = content_color);
                Text!("{end}", color = content_color);
            }
        );
    }
}

/// The default title of a [`DateRangePicker`], as its `title` parameter's default: it reads the
/// picker around it.
#[doc(hidden)]
pub fn default_date_range_picker_title() -> Option<Callback> {
    Some(compose_runtime::callback(|| {
        if let Some(scope) = LOCAL_DATE_RANGE_PICKER.current() {
            DateRangePickerDefaults::date_range_picker_title(
                scope.state.display_mode(),
                Modifier.padding((Dp(64.0), Dp(0.0), Dp(12.0), Dp(0.0))),
                scope.colors.title_content_color,
            );
        }
    }))
}

/// The default headline of a [`DateRangePicker`], as its `headline` parameter's default.
#[doc(hidden)]
pub fn default_date_range_picker_headline() -> Option<Callback> {
    Some(compose_runtime::callback(|| {
        if let Some(scope) = LOCAL_DATE_RANGE_PICKER.current() {
            DateRangePickerDefaults::date_range_picker_headline(
                scope.state.selected_start_date_millis(),
                scope.state.selected_end_date_millis(),
                scope.state.display_mode(),
                &*scope.date_formatter,
                Modifier.padding((Dp(64.0), Dp(0.0), Dp(12.0), Dp(12.0))),
                scope.colors.headline_content_color,
            );
        }
    }))
}

/// What the default slots read of the picker around them.
#[derive(Clone)]
struct DateRangePickerScope {
    state: DateRangePickerState,
    date_formatter: Rc<dyn DatePickerFormatter>,
    colors: DatePickerColors,
}

impl ComposeArg for DateRangePickerScope {
    fn same(&self, other: &Self) -> bool {
        self.state == other.state
            && Rc::ptr_eq(&self.date_formatter, &other.date_formatter)
            && self.colors == other.colors
    }
}

static LOCAL_DATE_RANGE_PICKER: CompositionLocal<Option<DateRangePickerScope>> =
    CompositionLocal::new_static("LocalDateRangePicker", || None);

/// A date range picker, as Compose's `DateRangePicker`: a header (`title`, `headline`, the mode's
/// toggle when `show_mode_toggle`), then the months or the two fields by the state's mode.
#[composable]
pub fn DateRangePicker(
    state: DateRangePickerState,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::DatePickerDefaults::remember_date_formatter())] date_formatter: Rc<
        dyn DatePickerFormatter,
    >,
    #[default(crate::DatePickerDefaults::colors())] colors: DatePickerColors,
    #[default(crate::default_date_range_picker_title())] title: Option<impl Fn() + 'static>,
    #[default(crate::default_date_range_picker_headline())] headline: Option<impl Fn() + 'static>,
    #[default(true)] show_mode_toggle: bool,
    #[default(crate::default_date_picker_focus_requester())] focus_requester: Option<
        FocusRequester,
    >,
) {
    let calendar_model = remember_keyed(state, || state.calendar_model());
    let top_target = remember(FocusRequester::new);
    let bottom_target = remember(FocusRequester::new);
    let scope = DateRangePickerScope { state, date_formatter: date_formatter.clone(), colors };
    let mode_toggle = if show_mode_toggle {
        Some(compose_runtime::callback(move || {
            DisplayModeToggleButton!(
                Modifier.padding((Dp(0.0), Dp(0.0), Dp(12.0), Dp(12.0))),
                state.display_mode(),
                move |mode| state.set_display_mode(mode),
                colors
            );
        }))
    } else {
        None
    };
    CompositionLocalProvider(LOCAL_DATE_RANGE_PICKER.provides(Some(scope)), || {
        DateEntryContainer!(
            modifier.clone(),
            title.clone(),
            headline.clone(),
            mode_toggle.clone(),
            colors,
            MaterialTheme::typography().title_large.clone(),
            // The token's 128 less 60: there are no "Save" and "X" buttons by default.
            Dp(68.0),
            Some(top_target),
            Some(bottom_target),
            {
                SwitchableDateEntryContent!(
                    state.selected_start_date_millis(),
                    state.selected_end_date_millis(),
                    state.displayed_month_millis(),
                    state.display_mode(),
                    move |start, end| state.set_selection(start, end),
                    move |month| state.set_displayed_month_millis(month),
                    calendar_model,
                    YearRange(state.year_range()),
                    date_formatter,
                    state.selectable_dates(),
                    colors,
                    focus_requester,
                    top_target,
                    bottom_target
                );
            }
        );
    });
}

/// As Compose's `SwitchableDateEntryContent` of this file: a fade, not `DatePicker`'s slide.
#[composable]
#[allow(clippy::too_many_arguments)]
fn SwitchableDateEntryContent(
    selected_start_date_millis: Option<i64>,
    selected_end_date_millis: Option<i64>,
    displayed_month_millis: i64,
    display_mode: DisplayMode,
    on_dates_selection_change: impl Fn(Option<i64>, Option<i64>) + 'static,
    on_displayed_month_change: impl Fn(i64) + 'static,
    calendar_model: CalendarModel,
    year_range: YearRange,
    date_formatter: Rc<dyn DatePickerFormatter>,
    selectable_dates: Rc<dyn SelectableDates>,
    colors: DatePickerColors,
    focus_requester: Option<FocusRequester>,
    top_target: FocusRequester,
    bottom_target: FocusRequester,
) {
    Crossfade!(
        display_mode,
        modifier = Modifier.semantics(|config| config.set_is_traversal_group(true)),
        animation_spec = MaterialTheme::motion_scheme().fast_effects_spec(),
        |mode| match mode {
            DisplayMode::Picker => {
                DateRangePickerContent!(
                    selected_start_date_millis,
                    selected_end_date_millis,
                    displayed_month_millis,
                    on_dates_selection_change,
                    on_displayed_month_change,
                    calendar_model,
                    year_range,
                    date_formatter,
                    selectable_dates,
                    colors,
                    top_target,
                    bottom_target
                );
            }
            DisplayMode::Input => {
                DateRangeInputContent!(
                    selected_start_date_millis,
                    selected_end_date_millis,
                    on_dates_selection_change,
                    calendar_model,
                    year_range,
                    date_formatter,
                    selectable_dates,
                    colors,
                    focus_requester
                );
            }
        }
    );
}

/// As Compose's `DateRangePickerContent`: Tab leaves the months for what's around the picker,
/// through the targets around them.
#[composable]
#[allow(clippy::too_many_arguments)]
fn DateRangePickerContent(
    selected_start_date_millis: Option<i64>,
    selected_end_date_millis: Option<i64>,
    displayed_month_millis: i64,
    on_dates_selection_change: impl Fn(Option<i64>, Option<i64>) + 'static,
    on_displayed_month_change: impl Fn(i64) + 'static,
    calendar_model: CalendarModel,
    year_range: YearRange,
    date_formatter: Rc<dyn DatePickerFormatter>,
    selectable_dates: Rc<dyn SelectableDates>,
    colors: DatePickerColors,
    top_target: FocusRequester,
    bottom_target: FocusRequester,
) {
    let displayed_month = calendar_model.get_month_of_millis(displayed_month_millis);
    let month_index = displayed_month.index_in(&year_range.0).max(0) as usize;
    let months_list_state = remember_lazy_list_state(month_index, 0);
    // Unlike `DatePicker`'s, this goes there during a scroll too.
    LaunchedEffect(month_index, async move {
        if months_list_state.first_visible_item_index() != month_index {
            let _ = months_list_state.scroll_to_item(month_index, 0).await;
        }
    });
    let focus_manager = LOCAL_FOCUS_MANAGER.current();
    Column!(
        modifier = Modifier
            .padding((DATE_PICKER_HORIZONTAL_PADDING, Dp(0.0)))
            .on_preview_key_event(move |event: KeyEvent| {
                if is_shift_tab(&event) {
                    top_target.request_focus();
                    focus_manager.move_focus(FocusDirection::Previous);
                    true
                } else if is_tab(&event) {
                    bottom_target.request_focus();
                    focus_manager.move_focus(FocusDirection::Next);
                    true
                } else {
                    false
                }
            }),
        {
            WeekDays!(colors, calendar_model);
            VerticalMonthsList!(
                months_list_state,
                selected_start_date_millis,
                selected_end_date_millis,
                on_dates_selection_change,
                on_displayed_month_change,
                calendar_model,
                year_range,
                date_formatter,
                selectable_dates,
                colors
            );
        }
    );
}

/// As Compose's `VerticalMonthsList`: every month of the years, under its name.
#[composable]
#[allow(clippy::too_many_arguments)]
fn VerticalMonthsList(
    lazy_list_state: LazyListState,
    selected_start_date_millis: Option<i64>,
    selected_end_date_millis: Option<i64>,
    on_dates_selection_change: impl Fn(Option<i64>, Option<i64>) + 'static,
    on_displayed_month_change: impl Fn(i64) + 'static,
    calendar_model: CalendarModel,
    year_range: YearRange,
    date_formatter: Rc<dyn DatePickerFormatter>,
    selectable_dates: Rc<dyn SelectableDates>,
    colors: DatePickerColors,
) {
    let today_millis = calendar_model.today().utc_time_millis;
    let first_month = calendar_model.get_month(*year_range.0.start(), 1);
    let count = number_of_months_in_range(&year_range.0);
    let coroutine_scope = remember_coroutine_scope();
    let actions = custom_scroll_actions(lazy_list_state, coroutine_scope);
    let on_date_selection_change = move |date_millis: i64| {
        let (start, end) = update_date_selection(
            date_millis,
            selected_start_date_millis,
            selected_end_date_millis,
        );
        on_dates_selection_change(start, end);
    };
    ProvideTextStyle!(MaterialTheme::typography().body_large.clone(), {
        LazyColumn!(
            // No scrolling to tell of: the custom actions go through the months.
            modifier = Modifier.semantics(|config| {
                config.set_vertical_scroll_axis_range(ScrollAxisRange {
                    value: 0.0,
                    max_value: 0.0,
                    reverse_scrolling: false,
                })
            }),
            state = lazy_list_state,
            |scope| {
                let calendar_model = calendar_model.clone();
                let on_date_selection_change = on_date_selection_change.clone();
                let date_formatter = date_formatter.clone();
                let selectable_dates = selectable_dates.clone();
                let actions = actions.clone();
                scope.items_indexed(vec![(); count], move |item_scope, index, _| {
                    let month = calendar_model.plus_months(&first_month, index as i32);
                    let locale = calendar_model.locale().clone();
                    let info = match (selected_start_date_millis, selected_end_date_millis) {
                        (Some(start), Some(end)) => SelectedRangeInfo::calculate_range_info(
                            &month,
                            &calendar_model.get_canonical_date(start),
                            &calendar_model.get_canonical_date(end),
                        ),
                        _ => None,
                    };
                    let actions = actions.clone();
                    let name = date_formatter
                        .format_month_year(Some(month.start_utc_time_millis), &locale)
                        .unwrap_or_else(|| "-".to_string());
                    Column!(modifier = Modifier.fill_parent_max_width(&item_scope), {
                        ProvideTextStyle!(MaterialTheme::typography().title_small.clone(), {
                            let actions = actions.clone();
                            Text!(
                                "{name}",
                                modifier = Modifier
                                    .padding((Dp(24.0), Dp(20.0), Dp(0.0), Dp(8.0)))
                                    .semantics(move |config| {
                                        config.set_custom_actions(actions.clone());
                                    }),
                                color = colors.subhead_content_color
                            );
                        });
                        Month!(
                            month,
                            on_date_selection_change,
                            today_millis,
                            selected_start_date_millis,
                            selected_end_date_millis,
                            info,
                            date_formatter,
                            selectable_dates,
                            colors,
                            locale,
                            lazy_list_state,
                            None,
                            || {}
                        );
                    });
                });
            }
        );
    });
    let first_year = *year_range.0.start();
    LaunchedEffect(lazy_list_state, async move {
        update_displayed_month(
            lazy_list_state,
            on_displayed_month_change,
            calendar_model,
            first_year,
        )
        .await;
    });
}

/// The selection a day's click makes, as Compose's `updateDateSelection`: an end on or after the
/// lone start, otherwise a new start.
fn update_date_selection(
    date_millis: i64,
    current_start_date_millis: Option<i64>,
    current_end_date_millis: Option<i64>,
) -> (Option<i64>, Option<i64>) {
    match (current_start_date_millis, current_end_date_millis) {
        (Some(start), None) if date_millis >= start => (Some(start), Some(date_millis)),
        _ => (Some(date_millis), None),
    }
}

/// The scroll's actions on a month's name, as Compose's `customScrollActions`.
fn custom_scroll_actions(
    state: LazyListState,
    coroutine_scope: CoroutineScope,
) -> Vec<CustomAccessibilityAction> {
    let scroll_by = move |delta: i32, can_scroll: bool| {
        if can_scroll {
            coroutine_scope.launch(async move {
                let index = state.first_visible_item_index() as i32 + delta;
                let _ = state.scroll_to_item(index.max(0) as usize, 0).await;
            });
        }
        can_scroll
    };
    vec![
        CustomAccessibilityAction::new("Scroll to show the previous month", move || {
            scroll_by(-1, state.can_scroll_backward())
        }),
        CustomAccessibilityAction::new("Scroll to show the next month", move || {
            scroll_by(1, state.can_scroll_forward())
        }),
    ]
}

/// Where a range's ends fall in the 7 by 6 grid of a month, as Compose's `SelectedRangeInfo`: one
/// goes with a month a whole range touches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct SelectedRangeInfo {
    /// The cell of the first selected day, as column and row.
    grid_start_coordinates: (i32, i32),
    /// The cell of the last selected day, as column and row.
    grid_end_coordinates: (i32, i32),
    first_is_selection_start: bool,
    last_is_selection_end: bool,
}

impl ComposeArg for SelectedRangeInfo {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl SelectedRangeInfo {
    /// As `calculateRangeInfo`: none for a month outside the range.
    fn calculate_range_info(
        month: &CalendarMonth,
        start_date: &CalendarDate,
        end_date: &CalendarDate,
    ) -> Option<SelectedRangeInfo> {
        if start_date.utc_time_millis > month.end_utc_time_millis()
            || end_date.utc_time_millis < month.start_utc_time_millis
        {
            return None;
        }
        let first_cell = month.days_from_start_of_week_to_first_of_month;
        let first_is_selection_start = start_date.utc_time_millis >= month.start_utc_time_millis;
        let last_is_selection_end = end_date.utc_time_millis <= month.end_utc_time_millis();
        let start_cell =
            first_cell + if first_is_selection_start { start_date.day_of_month - 1 } else { 0 };
        let end_cell = first_cell
            + if last_is_selection_end { end_date.day_of_month } else { month.number_of_days }
            - 1;
        Some(SelectedRangeInfo {
            grid_start_coordinates: (start_cell % DAYS_IN_WEEK, start_cell / DAYS_IN_WEEK),
            grid_end_coordinates: (end_cell % DAYS_IN_WEEK, end_cell / DAYS_IN_WEEK),
            first_is_selection_start,
            last_is_selection_end,
        })
    }
}

/// The rectangles behind a range's days, as Compose's `drawRangeBackground`: half a cell at the
/// range's ends, so its circles close it, and whole rows between. Rectangles are ordered left to
/// right here, where Compose's have a negative width in right to left.
pub(crate) fn draw_range_background(
    scope: &mut DrawScope<'_>,
    selected_range_info: &SelectedRangeInfo,
    color: Color,
) {
    let density = scope.density();
    let cell = density.to_px(RECOMMENDED_SIZE_FOR_ACCESSIBILITY);
    let state_layer_height = density.to_px(DATE_STATE_LAYER_HEIGHT);
    let state_layer_padding = (cell - state_layer_height) / 2.0;
    let width = scope.size().width;
    let days = DAYS_IN_WEEK as f32;
    let between = (width - days * cell) / days;

    let (x1, y1) = selected_range_info.grid_start_coordinates;
    let (x2, y2) = selected_range_info.grid_end_coordinates;
    let start_offset = if selected_range_info.first_is_selection_start { cell / 2.0 } else { 0.0 };
    let end_offset = if selected_range_info.last_is_selection_end { cell / 2.0 } else { cell };
    let mut start_x = x1 as f32 * (cell + between) + start_offset + between / 2.0;
    let mut end_x = x2 as f32 * (cell + between) + end_offset + between / 2.0;
    let start_y = y1 as f32 * cell + state_layer_padding;
    let end_y = y2 as f32 * cell + state_layer_padding;

    let is_rtl = scope.layout_direction() == LayoutDirection::Rtl;
    if is_rtl {
        start_x = width - start_x;
        end_x = width - end_x;
    }
    let (row_start, row_end) = if is_rtl { (width, 0.0) } else { (0.0, width) };
    let mut row = |from: f32, to: f32, top: f32| {
        scope.draw_rect(
            Rect::new(from.min(to), top, from.max(to), top + state_layer_height),
            Paint::fill(color),
        );
    };
    if y1 == y2 {
        row(start_x, end_x, start_y);
    } else {
        row(start_x, row_end, start_y);
        for y in y1 + 1..y2 {
            row(0.0, width, y as f32 * cell + state_layer_padding);
        }
        row(row_start, end_x, end_y);
    }
}

const DATE_STATE_LAYER_HEIGHT: Dp = Dp(40.0);

/// As Compose's `DateRangeInputContent`: a field for each date, each checked against the other.
#[composable]
#[allow(clippy::too_many_arguments)]
fn DateRangeInputContent(
    selected_start_date_millis: Option<i64>,
    selected_end_date_millis: Option<i64>,
    on_dates_selection_change: impl Fn(Option<i64>, Option<i64>) + 'static,
    calendar_model: CalendarModel,
    year_range: YearRange,
    date_formatter: Rc<dyn DatePickerFormatter>,
    selectable_dates: Rc<dyn SelectableDates>,
    colors: DatePickerColors,
    focus_requester: Option<FocusRequester>,
) {
    let date_input_format =
        remember_keyed(calendar_model.clone(), || calendar_model.get_date_input_format());
    let validator = Rc::new(DateInputValidator::new(
        year_range.0.clone(),
        selectable_dates,
        date_input_format.clone(),
        date_formatter,
    ));
    validator.current_start_date_millis.set(selected_start_date_millis);
    validator.current_end_date_millis.set(selected_end_date_millis);
    let pattern = date_input_format.pattern_with_delimiters.to_uppercase();
    let locale = calendar_model.locale().clone();
    let on_start = on_dates_selection_change.clone();
    let on_end = on_dates_selection_change;
    Row!(
        modifier = Modifier.padding((Dp(24.0), Dp(10.0), Dp(24.0), Dp(0.0))),
        horizontal_arrangement = Arrangement::spaced_by(Dp(8.0)),
        |scope| {
            let on_start = on_start.clone();
            let on_end = on_end.clone();
            DateInputTextField!(
                Modifier.weight(scope, 0.5),
                selected_start_date_millis,
                move |start| on_start(start, selected_end_date_millis),
                calendar_model.clone(),
                Some(date_field_label("Start date", pattern.clone())),
                Some(date_field_placeholder(pattern.clone())),
                InputIdentifier::Start,
                validator.clone(),
                date_input_format.clone(),
                locale.clone(),
                colors,
                focus_requester
            );
            DateInputTextField!(
                Modifier.weight(scope, 0.5),
                selected_end_date_millis,
                move |end| on_end(selected_start_date_millis, end),
                calendar_model.clone(),
                Some(date_field_label("End date", pattern.clone())),
                Some(date_field_placeholder(pattern.clone())),
                InputIdentifier::End,
                validator.clone(),
                date_input_format.clone(),
                locale.clone(),
                colors,
                // Only one field asks for the focus.
                None
            );
        }
    );
}

fn date_field_label(text: &'static str, pattern: String) -> Callback {
    compose_runtime::callback(move || {
        let description = format!("{text}, {pattern}");
        Text!(
            "{text}",
            modifier = Modifier
                .semantics(move |config| config.set_content_description(description.clone()))
        );
    })
}

fn date_field_placeholder(pattern: String) -> Callback {
    compose_runtime::callback(move || {
        Text!("{pattern}", modifier = Modifier.clear_and_set_semantics(|_| {}));
    })
}
