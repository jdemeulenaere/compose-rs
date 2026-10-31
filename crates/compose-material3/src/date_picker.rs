//! The date picker, as Compose's `DatePicker`: `DatePickerState`, `SelectableDates`,
//! `DatePickerFormatter`, `DisplayMode`, `DatePickerColors`, `DatePickerDefaults`, the months and
//! the years; `DatePickerDialog`. The date input is in `date_input.rs`.

use std::cell::Cell;
use std::ops::RangeInclusive;
use std::rc::Rc;

use compose_animation::{
    AnimatedContent, AnimatedVisibility, animate_color_as_state, expand_vertically_with,
    exponential_decay, fade_in_with, fade_out_with, shrink_vertically_with, size_transform,
    slide_in_vertically_with, slide_out_vertically_with,
};
use compose_foundation::gestures::{
    FlingBehavior, ScrollableState as _, SnapLayoutInfoProvider, SnapPosition,
    list_snap_layout_info_provider, snap_fling_behavior,
};
use compose_foundation::layout::{
    Arrangement, Box, Column, ColumnScope, HorizontalArrangement, Row, Spacer,
};
use compose_foundation::{
    BorderStroke, CIRCLE_SHAPE, GridCells, LazyItemModifiers, LazyListState, LazyRow,
    LazyVerticalGrid, remember_lazy_grid_state, remember_lazy_list_state,
};
use compose_runtime::{
    Callback, ComposeArg, CompositionLocal, CompositionLocalProvider, CoroutineScope,
    LaunchedEffect, ObjectHandle, Owned, OwnedState, composable, flow, mutable_state_of, remember,
    remember_coroutine_scope, remember_keyed, remember_saveable, snapshot_flow,
};
use compose_ui::{
    Alignment, Color, DialogProperties, Dp, FocusDirection, FocusManager, FocusRequester, Key,
    KeyEvent, KeyEventType, LOCAL_DENSITY, LOCAL_FOCUS_MANAGER, LOCAL_LAYOUT_DIRECTION,
    LayoutDirection, LiveRegionMode, Locale, Modifier, Role, ScrollAxisRange, Shape, SizeSpec,
    TextAlign, TextStyle,
};

use crate::button::{ButtonDefaults, TextButton};
use crate::calendar_model::{
    CalendarDate, CalendarModel, CalendarMonth, DAYS_IN_WEEK, MILLISECONDS_IN_24_HOURS,
    default_locale, format_with_skeleton, to_local_string,
};
use crate::components::{
    LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE, SelectableSurface, Surface, Text, TextFieldColors,
};
use crate::date_input::DateInputContent;
use crate::dialog::{BasicAlertDialog, alert_dialog_flow_row};
use crate::icon::{Icon, IconButton, icons};
use crate::scaffold::{DividerDefaults, HorizontalDivider};
use crate::theme::{
    LOCAL_CONTENT_COLOR, MaterialTheme, ProvideTextStyle, provide_content_color_text_style,
};
use crate::tooltip::{
    PlainTooltip, TooltipAnchorPosition, TooltipBox, TooltipDefaults, remember_tooltip_state,
};

/// What of a calendar can be picked, as Compose's `SelectableDates`.
pub trait SelectableDates {
    /// Whether a day, at its first millisecond, can be picked.
    fn is_selectable_date(&self, _utc_time_millis: i64) -> bool {
        true
    }

    /// Whether a year can be picked in the years' grid, and its days at all.
    fn is_selectable_year(&self, _year: i32) -> bool {
        true
    }
}

/// How a date picker writes dates, as Compose's `DatePickerFormatter`.
pub trait DatePickerFormatter {
    /// As `formatMonthYear`.
    fn format_month_year(&self, month_millis: Option<i64>, locale: &Locale) -> Option<String>;

    /// As `formatDate`; `for_content_description` asks for what's read out.
    fn format_date(
        &self,
        date_millis: Option<i64>,
        locale: &Locale,
        for_content_description: bool,
    ) -> Option<String>;
}

/// How a date is entered, as Compose's `DisplayMode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum DisplayMode {
    /// On a calendar, as `DisplayMode.Picker`.
    Picker,
    /// Typed, as `DisplayMode.Input`.
    Input,
}

impl ComposeArg for DisplayMode {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// `SelectableDates` in a state: the same object is the same value.
#[derive(Clone)]
pub(crate) struct SelectableDatesRef(pub(crate) Rc<dyn SelectableDates>);

impl PartialEq for SelectableDatesRef {
    fn eq(&self, other: &SelectableDatesRef) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// What the date picker's states share, as Compose's `BaseDatePickerStateImpl`.
pub(crate) struct BaseDatePickerState {
    pub(crate) calendar_model: CalendarModel,
    pub(crate) year_range: RangeInclusive<i32>,
    pub(crate) selectable_dates: OwnedState<SelectableDatesRef>,
    displayed_month: OwnedState<CalendarMonth>,
}

impl BaseDatePickerState {
    pub(crate) fn new(
        locale: Locale,
        initial_displayed_month_millis: Option<i64>,
        year_range: RangeInclusive<i32>,
        selectable_dates: Rc<dyn SelectableDates>,
    ) -> BaseDatePickerState {
        let calendar_model = CalendarModel::new(locale);
        let today = calendar_model.get_month_of_date(&calendar_model.today());
        let month = initial_displayed_month_millis
            .map(|millis| calendar_model.get_month_of_millis(millis))
            .filter(|month| year_range.contains(&month.year))
            .unwrap_or(today);
        BaseDatePickerState {
            calendar_model,
            year_range,
            selectable_dates: mutable_state_of(SelectableDatesRef(selectable_dates)),
            displayed_month: mutable_state_of(month),
        }
    }

    pub(crate) fn displayed_month_millis(&self) -> i64 {
        self.displayed_month.with(|month| month.start_utc_time_millis)
    }

    /// A month out of the years is ignored.
    pub(crate) fn set_displayed_month_millis(&self, month_millis: i64) {
        let month = self.calendar_model.get_month_of_millis(month_millis);
        if self.year_range.contains(&month.year) {
            self.displayed_month.set(month);
        }
    }
}

struct DatePickerStateInner {
    base: BaseDatePickerState,
    selected_date: OwnedState<Option<CalendarDate>>,
    display_mode: OwnedState<DisplayMode>,
}

/// A date picker's state, as Compose's `DatePickerState`: make one with [`DatePickerState::new`],
/// or use [`remember_date_picker_state`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DatePickerState {
    object: ObjectHandle<DatePickerStateInner>,
}

impl DatePickerState {
    /// Its object, or `None` once its owner went (D240): then it does nothing.
    #[track_caller]
    fn inner(&self) -> Option<Rc<DatePickerStateInner>> {
        self.object.get()
    }

    /// As `DatePickerState(locale, …)`: a selected date out of `year_range` is none; no displayed
    /// month is the selected date's month, and a month out of `year_range`, or none at all, the
    /// current one.
    #[track_caller]
    pub fn new(
        locale: Locale,
        initial_selected_date_millis: Option<i64>,
        initial_displayed_month_millis: Option<i64>,
        year_range: RangeInclusive<i32>,
        initial_display_mode: DisplayMode,
        selectable_dates: Rc<dyn SelectableDates>,
    ) -> Owned<DatePickerState> {
        let base = BaseDatePickerState::new(
            locale,
            initial_displayed_month_millis.or(initial_selected_date_millis),
            year_range,
            selectable_dates,
        );
        let selected = initial_selected_date_millis
            .map(|millis| base.calendar_model.get_canonical_date(millis))
            .filter(|date| base.year_range.contains(&date.year));
        Owned::new(
            DatePickerStateInner {
                base,
                selected_date: mutable_state_of(selected),
                display_mode: mutable_state_of(initial_display_mode),
            },
            |object| DatePickerState { object },
        )
    }

    /// The selected day's first millisecond, as `selectedDateMillis`.
    pub fn selected_date_millis(&self) -> Option<i64> {
        self.inner()?.selected_date.with(|date| date.map(|date| date.utc_time_millis))
    }

    /// As setting `selectedDateMillis`: rounded down to its day; a day out of the years is none.
    pub fn set_selected_date_millis(&self, date_millis: Option<i64>) {
        if let Some(inner) = self.inner() {
            let date = date_millis
                .map(|millis| inner.base.calendar_model.get_canonical_date(millis))
                .filter(|date| inner.base.year_range.contains(&date.year));
            inner.selected_date.set(date);
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

    /// As setting `displayMode`: the displayed month goes to the selected date's.
    pub fn set_display_mode(&self, display_mode: DisplayMode) {
        if let Some(inner) = self.inner() {
            if let Some(selected) = self.selected_date_millis() {
                inner.base.set_displayed_month_millis(
                    inner.base.calendar_model.get_month_of_millis(selected).start_utc_time_millis,
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

    pub(crate) fn set_selectable_dates(&self, selectable_dates: Rc<dyn SelectableDates>) {
        if let Some(inner) = self.inner() {
            inner.base.selectable_dates.set(SelectableDatesRef(selectable_dates));
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

    /// What's saved of a state, as `DatePickerStateImpl.Saver`: the selectable dates and the locale
    /// come back from the caller.
    fn saver(
        selectable_dates: Rc<dyn SelectableDates>,
        locale: Locale,
    ) -> impl compose_runtime::Saver<Owned<DatePickerState>> {
        use compose_runtime::Saveable;
        compose_runtime::saver(
            |state: &Owned<DatePickerState>| {
                state.inner()?;
                let range = state.year_range();
                let mode = i32::from(state.display_mode() == DisplayMode::Input);
                Some(
                    (
                        state.selected_date_millis(),
                        state.displayed_month_millis(),
                        (*range.start(), *range.end()),
                        mode,
                    )
                        .save(),
                )
            },
            move |saved| {
                <(Option<i64>, i64, (i32, i32), i32)>::restore(saved).map(
                    |(selected, displayed, (first, last), mode)| {
                        DatePickerState::new(
                            locale.clone(),
                            selected,
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

impl ComposeArg for DatePickerState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A date picker state remembered here and saved, in the default locale, as
/// `rememberDatePickerState`. `initial_displayed_month_millis` left out is the selected date's
/// month, as Compose's default, or else the current one; Compose's `null`, the current month with
/// a date selected, is today's millis given here.
#[composable(non_restartable)]
pub fn remember_date_picker_state(
    #[default(None)] initial_selected_date_millis: Option<i64>,
    #[default(None)] initial_displayed_month_millis: Option<i64>,
    #[default(crate::DatePickerDefaults::YEAR_RANGE)] year_range: RangeInclusive<i32>,
    #[default(crate::DisplayMode::Picker)] initial_display_mode: DisplayMode,
    #[default(crate::DatePickerDefaults::all_dates())] selectable_dates: Rc<dyn SelectableDates>,
) -> DatePickerState {
    let locale = default_locale();
    let saver = DatePickerState::saver(selectable_dates.clone(), locale.clone());
    let initial_selectable_dates = selectable_dates.clone();
    let state = compose_runtime::remember_saveable_with(saver, move || {
        DatePickerState::new(
            locale.clone(),
            initial_selected_date_millis,
            initial_displayed_month_millis,
            year_range.clone(),
            initial_display_mode,
            initial_selectable_dates.clone(),
        )
    });
    state.set_selectable_dates(selectable_dates);
    state
}

struct AllDates;
impl SelectableDates for AllDates {}

/// A date picker's defaults, as Compose's `DatePickerDefaults`.
pub struct DatePickerDefaults;

impl DatePickerDefaults {
    /// As `YearRange`.
    pub const YEAR_RANGE: RangeInclusive<i32> = 1900..=2100;
    /// As `TonalElevation`.
    pub const TONAL_ELEVATION: Dp = Dp(0.0);
    /// As `YearMonthSkeleton`.
    pub const YEAR_MONTH_SKELETON: &'static str = "yMMMM";
    /// As `YearAbbrMonthDaySkeleton`.
    pub const YEAR_ABBR_MONTH_DAY_SKELETON: &'static str = "yMMMd";
    /// As `YearMonthWeekdayDaySkeleton`.
    pub const YEAR_MONTH_WEEKDAY_DAY_SKELETON: &'static str = "yMMMMEEEEd";

    /// Every date, as `AllDates`.
    pub fn all_dates() -> Rc<dyn SelectableDates> {
        thread_local! {
            static ALL_DATES: Rc<dyn SelectableDates> = Rc::new(AllDates);
        }
        ALL_DATES.with(Rc::clone)
    }

    /// As `shape`.
    pub fn shape() -> Shape {
        MaterialTheme::shapes().extra_large.clone()
    }

    /// As `colors()`, from `DatePickerModalTokens`: change them with struct update.
    pub fn colors() -> DatePickerColors {
        let scheme = MaterialTheme::color_scheme();
        const DISABLED_ALPHA: f32 = 0.38;
        DatePickerColors {
            container_color: scheme.surface_container_high,
            title_content_color: scheme.on_surface_variant,
            headline_content_color: scheme.on_surface_variant,
            weekday_content_color: scheme.on_surface,
            subhead_content_color: scheme.on_surface_variant,
            navigation_content_color: scheme.on_surface_variant,
            year_content_color: scheme.on_surface_variant,
            disabled_year_content_color: scheme.on_surface_variant.with_alpha(DISABLED_ALPHA),
            current_year_content_color: scheme.primary,
            selected_year_content_color: scheme.on_primary,
            disabled_selected_year_content_color: scheme.on_primary.with_alpha(DISABLED_ALPHA),
            selected_year_container_color: scheme.primary,
            disabled_selected_year_container_color: scheme.primary.with_alpha(DISABLED_ALPHA),
            day_content_color: scheme.on_surface,
            disabled_day_content_color: scheme.on_surface.with_alpha(DISABLED_ALPHA),
            selected_day_content_color: scheme.on_primary,
            disabled_selected_day_content_color: scheme.on_primary.with_alpha(DISABLED_ALPHA),
            selected_day_container_color: scheme.primary,
            disabled_selected_day_container_color: scheme.primary.with_alpha(DISABLED_ALPHA),
            today_content_color: scheme.primary,
            today_date_border_color: scheme.primary,
            day_in_selection_range_content_color: scheme.on_secondary_container,
            day_in_selection_range_container_color: scheme.secondary_container,
            divider_color: scheme.outline_variant,
            date_text_field_colors: crate::OutlinedTextFieldDefaults::colors(),
        }
    }

    /// As `dateFormatter(…)`, of the three skeletons.
    pub fn date_formatter(
        year_selection_skeleton: &'static str,
        selected_date_skeleton: &'static str,
        selected_date_description_skeleton: &'static str,
    ) -> Rc<dyn DatePickerFormatter> {
        Rc::new(DatePickerFormatterImpl {
            year_selection_skeleton,
            selected_date_skeleton,
            selected_date_description_skeleton,
        })
    }

    /// `dateFormatter()` remembered here, as `DatePicker`'s default.
    pub fn remember_date_formatter() -> Rc<dyn DatePickerFormatter> {
        remember(|| {
            DatePickerDefaults::date_formatter(
                DatePickerDefaults::YEAR_MONTH_SKELETON,
                DatePickerDefaults::YEAR_ABBR_MONTH_DAY_SKELETON,
                DatePickerDefaults::YEAR_MONTH_WEEKDAY_DAY_SKELETON,
            )
        })
    }

    /// The title, by the mode, as `DatePickerTitle`.
    pub fn date_picker_title(display_mode: DisplayMode, modifier: Modifier, content_color: Color) {
        let text = match display_mode {
            DisplayMode::Picker => "Select date",
            DisplayMode::Input => "Select date",
        };
        Text!("{text}", modifier = modifier, color = content_color);
    }

    /// The headline, the selected date or what says there's none, as `DatePickerHeadline`.
    pub fn date_picker_headline(
        selected_date_millis: Option<i64>,
        display_mode: DisplayMode,
        date_formatter: &dyn DatePickerFormatter,
        modifier: Modifier,
        content_color: Color,
    ) {
        let locale = default_locale();
        let formatted_date = date_formatter.format_date(selected_date_millis, &locale, false);
        let verbose = date_formatter
            .format_date(selected_date_millis, &locale, true)
            .unwrap_or_else(|| "None".to_string());
        let headline = formatted_date.unwrap_or_else(|| match display_mode {
            DisplayMode::Picker => "Selected date".to_string(),
            DisplayMode::Input => "Entered date".to_string(),
        });
        let description = match display_mode {
            DisplayMode::Picker => format!("Current selection: {verbose}"),
            DisplayMode::Input => format!("Entered date: {verbose}"),
        };
        Text!(
            "{headline}",
            modifier = modifier.semantics(move |config| {
                config.set_live_region(LiveRegionMode::Polite);
                config.set_content_description(description.clone());
            }),
            color = content_color,
            max_lines = 1
        );
    }

    /// A fling that lands on the next item or stays, as `rememberSnapFlingBehavior`: it doesn't
    /// approach, so it goes a month at a time.
    pub(crate) fn remember_snap_fling_behavior(state: LazyListState) -> Rc<dyn FlingBehavior> {
        let density = LOCAL_DENSITY.current();
        let spec = MaterialTheme::motion_scheme().default_effects_spec();
        remember_keyed((state, density), move || {
            struct NoApproach(Rc<dyn SnapLayoutInfoProvider>);
            impl SnapLayoutInfoProvider for NoApproach {
                fn calculate_approach_offset(&self, _velocity: f32, _decay_offset: f32) -> f32 {
                    0.0
                }
                fn calculate_snap_offset(&self, velocity: f32) -> f32 {
                    self.0.calculate_snap_offset(velocity)
                }
                fn is_layout_behind(&self) -> bool {
                    self.0.is_layout_behind()
                }
            }
            let provider = list_snap_layout_info_provider(state, SnapPosition::Center, density);
            Rc::new(snap_fling_behavior(
                Rc::new(NoApproach(provider)),
                exponential_decay(1.0, 0.1),
                spec.clone(),
            )) as Rc<dyn FlingBehavior>
        })
    }
}

/// The default title of a [`DatePicker`], as its `title` parameter's default: it reads the picker
/// around it.
#[doc(hidden)]
pub fn default_date_picker_title() -> Option<Callback> {
    Some(compose_runtime::callback(|| {
        if let Some(scope) = LOCAL_DATE_PICKER.current() {
            DatePickerDefaults::date_picker_title(
                scope.state.display_mode(),
                Modifier.padding((Dp(24.0), Dp(16.0), Dp(12.0), Dp(0.0))),
                scope.colors.title_content_color,
            );
        }
    }))
}

/// The default headline of a [`DatePicker`], as its `headline` parameter's default.
#[doc(hidden)]
pub fn default_date_picker_headline() -> Option<Callback> {
    Some(compose_runtime::callback(|| {
        if let Some(scope) = LOCAL_DATE_PICKER.current() {
            DatePickerDefaults::date_picker_headline(
                scope.state.selected_date_millis(),
                scope.state.display_mode(),
                &*scope.date_formatter,
                Modifier.padding((Dp(24.0), Dp(0.0), Dp(12.0), Dp(12.0))),
                scope.colors.headline_content_color,
            );
        }
    }))
}

/// A requester remembered here, as `DatePicker`'s `focus_requester` parameter's default.
#[doc(hidden)]
pub fn default_date_picker_focus_requester() -> Option<FocusRequester> {
    Some(remember(FocusRequester::new))
}

/// What the default slots read of the picker around them.
#[derive(Clone)]
struct DatePickerScope {
    state: DatePickerState,
    date_formatter: Rc<dyn DatePickerFormatter>,
    colors: DatePickerColors,
}

impl ComposeArg for DatePickerScope {
    fn same(&self, other: &Self) -> bool {
        self.state == other.state
            && Rc::ptr_eq(&self.date_formatter, &other.date_formatter)
            && self.colors == other.colors
    }
}

static LOCAL_DATE_PICKER: CompositionLocal<Option<DatePickerScope>> =
    CompositionLocal::new_static("LocalDatePicker", || None);

/// A date picker's colors, as Compose's `DatePickerColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DatePickerColors {
    pub container_color: Color,
    pub title_content_color: Color,
    pub headline_content_color: Color,
    pub weekday_content_color: Color,
    pub subhead_content_color: Color,
    pub navigation_content_color: Color,
    pub year_content_color: Color,
    pub disabled_year_content_color: Color,
    pub current_year_content_color: Color,
    pub selected_year_content_color: Color,
    pub disabled_selected_year_content_color: Color,
    pub selected_year_container_color: Color,
    pub disabled_selected_year_container_color: Color,
    pub day_content_color: Color,
    pub disabled_day_content_color: Color,
    pub selected_day_content_color: Color,
    pub disabled_selected_day_content_color: Color,
    pub selected_day_container_color: Color,
    pub disabled_selected_day_container_color: Color,
    pub today_content_color: Color,
    pub today_date_border_color: Color,
    pub day_in_selection_range_content_color: Color,
    pub day_in_selection_range_container_color: Color,
    pub divider_color: Color,
    pub date_text_field_colors: TextFieldColors,
}

impl ComposeArg for DatePickerColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl DatePickerColors {
    fn day_content_color_target(
        &self,
        is_today: bool,
        selected: bool,
        in_range: bool,
        enabled: bool,
    ) -> Color {
        match () {
            _ if selected && enabled => self.selected_day_content_color,
            _ if selected => self.disabled_selected_day_content_color,
            _ if in_range && enabled => self.day_in_selection_range_content_color,
            _ if in_range => self.disabled_day_content_color,
            _ if is_today && enabled => self.today_content_color,
            _ if enabled => self.day_content_color,
            _ => self.disabled_day_content_color,
        }
    }

    fn day_container_color_target(&self, selected: bool, enabled: bool) -> Color {
        match (selected, enabled) {
            (true, true) => self.selected_day_container_color,
            (true, false) => self.disabled_selected_day_container_color,
            _ => Color::TRANSPARENT,
        }
    }

    fn year_content_color_target(
        &self,
        current_year: bool,
        selected: bool,
        enabled: bool,
    ) -> Color {
        match () {
            _ if selected && enabled => self.selected_year_content_color,
            _ if selected => self.disabled_selected_year_content_color,
            _ if current_year && enabled => self.current_year_content_color,
            _ if enabled => self.year_content_color,
            _ => self.disabled_year_content_color,
        }
    }

    fn year_container_color_target(&self, selected: bool, enabled: bool) -> Color {
        match (selected, enabled) {
            (true, true) => self.selected_year_container_color,
            (true, false) => self.disabled_selected_year_container_color,
            _ => Color::TRANSPARENT,
        }
    }
}

struct DatePickerFormatterImpl {
    year_selection_skeleton: &'static str,
    selected_date_skeleton: &'static str,
    selected_date_description_skeleton: &'static str,
}

impl DatePickerFormatter for DatePickerFormatterImpl {
    fn format_month_year(&self, month_millis: Option<i64>, locale: &Locale) -> Option<String> {
        Some(format_with_skeleton(month_millis?, self.year_selection_skeleton, locale))
    }

    fn format_date(
        &self,
        date_millis: Option<i64>,
        locale: &Locale,
        for_content_description: bool,
    ) -> Option<String> {
        let skeleton = if for_content_description {
            self.selected_date_description_skeleton
        } else {
            self.selected_date_skeleton
        };
        Some(format_with_skeleton(date_millis?, skeleton, locale))
    }
}

/// A date picker, as Compose's `DatePicker`: a header (`title`, `headline`, the mode's toggle when
/// `show_mode_toggle`), then the calendar or the date input by the state's mode.
#[composable]
pub fn DatePicker(
    state: DatePickerState,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::DatePickerDefaults::remember_date_formatter())] date_formatter: Rc<
        dyn DatePickerFormatter,
    >,
    #[default(crate::DatePickerDefaults::colors())] colors: DatePickerColors,
    #[default(crate::default_date_picker_title())] title: Option<impl Fn() + 'static>,
    #[default(crate::default_date_picker_headline())] headline: Option<impl Fn() + 'static>,
    #[default(true)] show_mode_toggle: bool,
    #[default(crate::default_date_picker_focus_requester())] focus_requester: Option<
        FocusRequester,
    >,
) {
    let calendar_model = remember_keyed(state, || state.calendar_model());
    let scope = DatePickerScope { state, date_formatter: date_formatter.clone(), colors };
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
    CompositionLocalProvider(LOCAL_DATE_PICKER.provides(Some(scope)), || {
        DateEntryContainer!(
            modifier.clone(),
            title.clone(),
            headline.clone(),
            mode_toggle.clone(),
            colors,
            MaterialTheme::typography().headline_large.clone(),
            Dp(120.0),
            {
                SwitchableDateEntryContent!(
                    state.selected_date_millis(),
                    state.displayed_month_millis(),
                    state.display_mode(),
                    move |date| state.set_selected_date_millis(date),
                    move |month| state.set_displayed_month_millis(month),
                    calendar_model,
                    YearRange(state.year_range()),
                    date_formatter,
                    state.selectable_dates(),
                    colors,
                    focus_requester
                );
            }
        );
    });
}

/// A year range as an argument of a composable.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct YearRange(pub(crate) RangeInclusive<i32>);

impl ComposeArg for YearRange {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// `Modifier.size` of the minimum touch target, when there's one.
fn minimum_size(modifier: Modifier) -> Modifier {
    let minimum = LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.current();
    if minimum.is_specified() { modifier.size((minimum, minimum)) } else { modifier }
}

/// As Compose's `DateEntryContainer`.
#[composable]
#[allow(clippy::too_many_arguments)]
pub(crate) fn DateEntryContainer(
    modifier: Modifier,
    title: Option<impl Fn() + 'static>,
    headline: Option<impl Fn() + 'static>,
    mode_toggle_button: Option<impl Fn() + 'static>,
    colors: DatePickerColors,
    headline_text_style: TextStyle,
    header_min_height: Dp,
    content: impl Fn() + 'static,
) {
    Column!(
        modifier = modifier
            .size_in(SizeSpec::bounds(
                CONTAINER_WIDTH,
                Dp::UNSPECIFIED,
                Dp::UNSPECIFIED,
                Dp::UNSPECIFIED
            ))
            .semantics(|config| config.set_is_traversal_group(true))
            .background(colors.container_color),
        {
            DatePickerHeader!(
                Modifier,
                title.clone(),
                colors.title_content_color,
                colors.headline_content_color,
                header_min_height,
                {
                    Column!(modifier = Modifier.fill_max_width(), {
                        Row!(
                            modifier = Modifier.fill_max_width(),
                            horizontal_arrangement =
                                match (headline.is_some(), mode_toggle_button.is_some()) {
                                    (true, true) =>
                                        HorizontalArrangement::from(Arrangement::SPACE_BETWEEN),
                                    (true, false) => Arrangement::START,
                                    _ => Arrangement::END,
                                },
                            vertical_alignment = Alignment::CENTER_VERTICALLY,
                            |scope| {
                                if let Some(headline) = &headline {
                                    let headline = headline.clone();
                                    ProvideTextStyle!(headline_text_style.clone(), {
                                        Box!(modifier = Modifier.weight(scope, 1.0), {
                                            headline();
                                        });
                                    });
                                }
                                if let Some(toggle) = &mode_toggle_button {
                                    toggle();
                                }
                            }
                        );
                        if title.is_some() || headline.is_some() || mode_toggle_button.is_some() {
                            HorizontalDivider!(color = colors.divider_color);
                        }
                    });
                }
            );
            content();
        }
    );
}

/// As Compose's `DatePickerHeader`.
#[composable]
pub(crate) fn DatePickerHeader(
    modifier: Modifier,
    title: Option<impl Fn() + 'static>,
    title_content_color: Color,
    headline_content_color: Color,
    min_height: Dp,
    content: impl Fn() + 'static,
) {
    let height_modifier = if title.is_some() {
        Modifier.default_min_size(Dp::UNSPECIFIED, min_height)
    } else {
        Modifier
    };
    Column!(
        modifier = modifier.fill_max_width().then(height_modifier),
        vertical_arrangement = Arrangement::SPACE_BETWEEN,
        {
            if let Some(title) = &title {
                let title = title.clone();
                let style = MaterialTheme::typography().label_large.clone();
                provide_content_color_text_style(title_content_color, style, || {
                    Box!(content_alignment = Alignment::BOTTOM_START, {
                        title();
                    });
                });
            }
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(headline_content_color), || {
                content();
            });
        }
    );
}

/// As Compose's `DisplayModeToggleButton`.
#[composable]
pub(crate) fn DisplayModeToggleButton(
    modifier: Modifier,
    display_mode: DisplayMode,
    on_display_mode_change: impl Fn(DisplayMode) + 'static,
    colors: DatePickerColors,
) {
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(colors.headline_content_color), || {
        let (next, icon, description) = match display_mode {
            DisplayMode::Picker => {
                (DisplayMode::Input, icons::filled::edit(), "Switch to text input mode")
            }
            DisplayMode::Input => {
                (DisplayMode::Picker, icons::filled::date_range(), "Switch to calendar input mode")
            }
        };
        IconButtonWithTooltip!(
            move || on_display_mode_change(next),
            icon,
            description,
            modifier.clone(),
            true
        );
    });
}

/// As Compose's `SwitchableDateEntryContent`: the calendar and the input give way to each other,
/// sliding and fading, the size following.
#[composable]
#[allow(clippy::too_many_arguments)]
fn SwitchableDateEntryContent(
    selected_date_millis: Option<i64>,
    displayed_month_millis: i64,
    display_mode: DisplayMode,
    on_date_selection_change: impl Fn(Option<i64>) + 'static,
    on_displayed_month_change: impl Fn(i64) + 'static,
    calendar_model: CalendarModel,
    year_range: YearRange,
    date_formatter: Rc<dyn DatePickerFormatter>,
    selectable_dates: Rc<dyn SelectableDates>,
    colors: DatePickerColors,
    focus_requester: Option<FocusRequester>,
) {
    let parallax_target = -LOCAL_DENSITY.current().round_to_px(Dp(48.0));
    let motion = MaterialTheme::motion_scheme();
    let (effects_in, effects_out) = (motion.default_effects_spec(), motion.fast_effects_spec());
    let spatial = motion.default_spatial_spec();
    AnimatedContent!(
        display_mode,
        modifier = Modifier.semantics(|config| config.set_is_traversal_group(true)),
        transition_spec = move |scope| {
            let spatial = spatial.clone();
            let size_spec = spatial.clone();
            let transform = if *scope.target_state() == DisplayMode::Input {
                (slide_in_vertically_with(spatial.clone(), |height| height)
                    + fade_in_with(effects_in.clone(), 0.0))
                .together_with(
                    fade_out_with(effects_out.clone(), 0.0)
                        + slide_out_vertically_with(spatial, move |_| parallax_target),
                )
            } else {
                (slide_in_vertically_with(spatial.clone(), move |_| parallax_target)
                    + fade_in_with(effects_in.clone(), 0.0))
                .together_with(
                    slide_out_vertically_with(spatial, |height| height)
                        + fade_out_with(effects_out.clone(), 0.0),
                )
            };
            transform.using(Some(size_transform(true, move |_, _| size_spec.clone())))
        },
        label = "DatePickerDisplayModeAnimation",
        |_, mode| match mode {
            DisplayMode::Picker => {
                DatePickerContent!(
                    selected_date_millis,
                    displayed_month_millis,
                    move |date| on_date_selection_change(Some(date)),
                    on_displayed_month_change,
                    calendar_model,
                    year_range,
                    date_formatter,
                    selectable_dates,
                    colors
                );
            }
            DisplayMode::Input => {
                DateInputContent!(
                    selected_date_millis,
                    on_date_selection_change,
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

/// As Compose's `DatePickerContent`.
#[composable]
#[allow(clippy::too_many_arguments)]
fn DatePickerContent(
    selected_date_millis: Option<i64>,
    displayed_month_millis: i64,
    on_date_selection_change: impl Fn(i64) + 'static,
    on_displayed_month_change: impl Fn(i64) + 'static,
    calendar_model: CalendarModel,
    year_range: YearRange,
    date_formatter: Rc<dyn DatePickerFormatter>,
    selectable_dates: Rc<dyn SelectableDates>,
    colors: DatePickerColors,
) {
    let displayed_month = calendar_model.get_month_of_millis(displayed_month_millis);
    let month_index = displayed_month.index_in(&year_range.0).max(0) as usize;
    let months_list_state = remember_lazy_list_state(month_index, 0);
    LaunchedEffect(month_index, async move {
        if !months_list_state.is_scroll_in_progress()
            && months_list_state.first_visible_item_index() != month_index
        {
            let _ = months_list_state.scroll_to_item(month_index, 0).await;
        }
    });
    let coroutine_scope = remember_coroutine_scope();
    let year_picker_visible = remember_saveable(|| mutable_state_of(false));
    let focus_manager = LOCAL_FOCUS_MANAGER.current();
    let next_button = remember(FocusRequester::new);
    let year_selection_button = remember(FocusRequester::new);
    let current_year = remember(FocusRequester::new);
    let divider = remember(FocusRequester::new);
    let total_months = number_of_months_in_range(&year_range.0);
    let scroll_by = move |delta: i32| {
        coroutine_scope.launch(async move {
            let index = months_list_state.first_visible_item_index() as i32 + delta;
            if index >= 0 {
                let _ = months_list_state.animate_scroll_to_item(index as usize, 0).await;
            }
        });
    };
    Column!({
        MonthsNavigation!(
            Modifier.padding((DATE_PICKER_HORIZONTAL_PADDING, Dp(0.0))),
            month_index + 1 < total_months,
            month_index > 0,
            year_picker_visible.get(),
            date_formatter
                .format_month_year(Some(displayed_month_millis), calendar_model.locale())
                .unwrap_or_else(|| "-".to_string()),
            Modifier.focus_requester(next_button),
            move || scroll_by(1),
            move || scroll_by(-1),
            move || year_picker_visible.set(!year_picker_visible.get()),
            move || {
                if !current_year.request_focus() {
                    focus_manager.move_focus(FocusDirection::Down);
                }
            },
            year_selection_button,
            colors
        );
        Box!({
            Column!(modifier = Modifier.padding((DATE_PICKER_HORIZONTAL_PADDING, Dp(0.0))), {
                WeekDays!(colors, calendar_model);
                HorizontalMonthsList!(
                    months_list_state,
                    selected_date_millis,
                    on_date_selection_change,
                    on_displayed_month_change,
                    calendar_model,
                    year_range,
                    date_formatter,
                    selectable_dates,
                    colors,
                    move || {
                        next_button.request_focus();
                    },
                    focus_manager
                );
            });
            let motion = MaterialTheme::motion_scheme();
            let shrink_expand = motion.default_effects_spec();
            AnimatedVisibility!(
                year_picker_visible.get(),
                modifier = Modifier.clip_to_bounds(),
                enter =
                    expand_vertically_with(shrink_expand.clone(), Alignment::BOTTOM, true, |_| 0)
                        + fade_in_with(motion.default_effects_spec(), 0.6),
                exit =
                    shrink_vertically_with(shrink_expand.clone(), Alignment::BOTTOM, true, |_| 0)
                        + fade_out_with(motion.fast_effects_spec(), 0.0),
                {
                    Column!(
                        modifier = Modifier
                            .semantics(|config| config.set_pane_title("Year picker visible")),
                        {
                            YearPicker!(
                                Modifier
                                    .required_height(Dp(RECOMMENDED_SIZE_FOR_ACCESSIBILITY.0
                                        * (MAX_CALENDAR_ROWS + 1) as f32
                                        - DividerDefaults::THICKNESS.0,))
                                    .padding((DATE_PICKER_HORIZONTAL_PADDING, Dp(0.0))),
                                displayed_month_millis,
                                move |year| {
                                    year_picker_visible.set(!year_picker_visible.get());
                                    let index = (year - *year_range.0.start()) * 12
                                        + displayed_month.month
                                        - 1;
                                    coroutine_scope.launch(async move {
                                        let _ = months_list_state
                                            .scroll_to_item(index as usize, 0)
                                            .await;
                                    });
                                },
                                selectable_dates,
                                calendar_model,
                                year_range,
                                colors,
                                current_year,
                                move || {
                                    year_selection_button.request_focus();
                                },
                                move || {
                                    divider.request_focus();
                                    focus_manager.move_focus(FocusDirection::Next);
                                }
                            );
                            HorizontalDivider!(
                                modifier = Modifier
                                    .focus_requester(divider)
                                    .on_key_event(move |event: KeyEvent| {
                                        divider_key_event(&event, &focus_manager, current_year)
                                    })
                                    .focus_target(),
                                color = colors.divider_color
                            );
                        }
                    );
                }
            );
        });
    });
}

/// What the divider under the years does with a key, as `DatePickerContent`'s: it isn't where the
/// focus stays.
fn divider_key_event(event: &KeyEvent, focus: &FocusManager, current_year: FocusRequester) -> bool {
    match event.key {
        Key::DirectionUp => {
            focus.move_focus(FocusDirection::Previous);
            true
        }
        Key::Tab if event.modifiers.shift => {
            current_year.request_focus();
            true
        }
        Key::DirectionDown | Key::Tab => {
            focus.move_focus(FocusDirection::Next);
            true
        }
        _ => false,
    }
}

/// As Compose's `HorizontalMonthsList`.
#[composable]
#[allow(clippy::too_many_arguments)]
fn HorizontalMonthsList(
    lazy_list_state: LazyListState,
    selected_date_millis: Option<i64>,
    on_date_selection_change: impl Fn(i64) + 'static,
    on_displayed_month_change: impl Fn(i64) + 'static,
    calendar_model: CalendarModel,
    year_range: YearRange,
    date_formatter: Rc<dyn DatePickerFormatter>,
    selectable_dates: Rc<dyn SelectableDates>,
    colors: DatePickerColors,
    on_return_focus: impl Fn() + 'static,
    focus_manager: FocusManager,
) {
    let today_millis = calendar_model.today().utc_time_millis;
    let first_month = calendar_model.get_month(*year_range.0.start(), 1);
    let count = number_of_months_in_range(&year_range.0);
    ProvideTextStyle!(MaterialTheme::typography().body_large.clone(), {
        LazyRow!(
            // No scrolling to tell of: the buttons go through the months.
            modifier = Modifier.semantics(|config| {
                config.set_horizontal_scroll_axis_range(ScrollAxisRange {
                    value: 0.0,
                    max_value: 0.0,
                    reverse_scrolling: false,
                })
            }),
            state = lazy_list_state,
            fling_behavior =
                Some(DatePickerDefaults::remember_snap_fling_behavior(lazy_list_state)),
            |scope| {
                let calendar_model = calendar_model.clone();
                let on_date_selection_change = on_date_selection_change.clone();
                let date_formatter = date_formatter.clone();
                let selectable_dates = selectable_dates.clone();
                let on_return_focus = on_return_focus.clone();
                let focus_manager = focus_manager.clone();
                scope.items_indexed(vec![(); count], move |item_scope, index, _| {
                    let month = calendar_model.plus_months(&first_month, index as i32);
                    let locale = calendar_model.locale().clone();
                    Box!(modifier = Modifier.fill_parent_max_width(&item_scope), {
                        Month!(
                            month,
                            on_date_selection_change,
                            today_millis,
                            selected_date_millis,
                            None,
                            false,
                            date_formatter,
                            selectable_dates,
                            colors,
                            locale,
                            lazy_list_state,
                            Some(focus_manager.clone()),
                            on_return_focus
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

/// Tells of the month scrolled to, as Compose's `updateDisplayedMonth`.
pub(crate) async fn update_displayed_month(
    lazy_list_state: LazyListState,
    on_displayed_month_change: Callback<dyn Fn(i64)>,
    calendar_model: CalendarModel,
    first_year: i32,
) {
    let mut indices = snapshot_flow(move || lazy_list_state.first_visible_item_index());
    while let Some(index) = flow::next(&mut indices).await {
        let (year_offset, month) = ((index / 12) as i32, (index % 12) as i32 + 1);
        on_displayed_month_change(
            calendar_model.get_month(first_year + year_offset, month).start_utc_time_millis,
        );
    }
}

/// As Compose's `WeekDays`: the narrow names, read out as the full ones, from the first day.
#[composable]
pub(crate) fn WeekDays(colors: DatePickerColors, calendar_model: CalendarModel) {
    let first_day = calendar_model.first_day_of_week() as usize;
    let names = calendar_model.weekday_names();
    let style = MaterialTheme::typography().body_large.clone();
    Row!(
        modifier = Modifier
            .default_min_size(Dp::UNSPECIFIED, RECOMMENDED_SIZE_FOR_ACCESSIBILITY)
            .fill_max_width(),
        horizontal_arrangement = Arrangement::SPACE_EVENLY,
        vertical_alignment = Alignment::CENTER_VERTICALLY,
        {
            for offset in 0..names.len() {
                let (full, narrow) = names[(first_day - 1 + offset) % names.len()];
                Box!(
                    modifier = minimum_size(
                        Modifier
                            .clear_and_set_semantics(move |config| {
                                config.set_content_description(full);
                            })
                            .size_in(day_bounds()),
                    ),
                    content_alignment = Alignment::CENTER,
                    {
                        Text!(
                            "{narrow}",
                            modifier = Modifier.wrap_content_size(),
                            color = colors.weekday_content_color,
                            text_align = Some(TextAlign::Center),
                            style = style.clone()
                        );
                    }
                );
            }
        }
    );
}

fn day_bounds() -> SizeSpec {
    SizeSpec::bounds(DATE_CONTAINER_SIZE, DATE_CONTAINER_SIZE, Dp::UNSPECIFIED, Dp::UNSPECIFIED)
}

/// As Compose's `Month`: six rows of seven cells, a spacer around its days. `range_selection`
/// says the two dates are a range's (the date range picker's).
#[composable]
#[allow(clippy::too_many_arguments)]
pub(crate) fn Month(
    month: CalendarMonth,
    on_date_selection_change: impl Fn(i64) + 'static,
    today_millis: i64,
    start_date_millis: Option<i64>,
    end_date_millis: Option<i64>,
    range_selection: bool,
    date_formatter: Rc<dyn DatePickerFormatter>,
    selectable_dates: Rc<dyn SelectableDates>,
    colors: DatePickerColors,
    locale: Locale,
    lazy_list_state: LazyListState,
    focus_manager: Option<FocusManager>,
    on_return_focus: impl Fn() + 'static,
) {
    let coroutine_scope = remember_coroutine_scope();
    let is_rtl = LOCAL_LAYOUT_DIRECTION.current() == LayoutDirection::Rtl;
    let first_enabled = first_enabled_day(&month, &*selectable_dates);
    let last_enabled = last_enabled_day(&month, &*selectable_dates);
    let first_cell = month.days_from_start_of_week_to_first_of_month;
    let end_cell = first_cell + month.number_of_days;
    Column!(
        modifier = Modifier
            .required_height(Dp(RECOMMENDED_SIZE_FOR_ACCESSIBILITY.0 * MAX_CALENDAR_ROWS as f32)),
        vertical_arrangement = Arrangement::SPACE_EVENLY,
        {
            for week in 0..MAX_CALENDAR_ROWS {
                Row!(
                    modifier = Modifier.fill_max_width(),
                    horizontal_arrangement = Arrangement::SPACE_EVENLY,
                    vertical_alignment = Alignment::CENTER_VERTICALLY,
                    {
                        for day_index in 0..DAYS_IN_WEEK {
                            let cell = week * DAYS_IN_WEEK + day_index;
                            if cell < first_cell || cell >= end_cell {
                                Spacer!(modifier = minimum_size(Modifier.size_in(day_bounds())));
                                continue;
                            }
                            let day_number = cell - first_cell;
                            let date_millis = month.start_utc_time_millis
                                + i64::from(day_number) * MILLISECONDS_IN_24_HOURS;
                            let is_today = date_millis == today_millis;
                            let start_selected = Some(date_millis) == start_date_millis;
                            let end_selected = Some(date_millis) == end_date_millis;
                            let in_range = range_selection
                                && date_millis >= start_date_millis.unwrap_or(i64::MAX)
                                && date_millis <= end_date_millis.unwrap_or(i64::MIN);
                            let state_description = day_content_description(
                                range_selection,
                                is_today,
                                start_selected,
                                end_selected,
                                in_range,
                            );
                            let formatted = date_formatter
                                .format_date(Some(date_millis), &locale, true)
                                .unwrap_or_default();
                            let enabled = selectable_dates.is_selectable_year(month.year)
                                && selectable_dates.is_selectable_date(date_millis);
                            Day!(
                                to_local_string(day_number + 1, 1, 40, &locale),
                                day_on_key_event(
                                    is_rtl,
                                    cell == first_enabled,
                                    cell == last_enabled,
                                    lazy_list_state,
                                    coroutine_scope,
                                    focus_manager.clone(),
                                    on_return_focus.clone(),
                                ),
                                start_selected || end_selected,
                                move || on_date_selection_change(date_millis),
                                start_selected,
                                enabled,
                                is_today,
                                in_range,
                                match state_description {
                                    Some(description) => format!("{description}, {formatted}"),
                                    None => formatted,
                                },
                                colors
                            );
                        }
                    }
                );
            }
        }
    );
}

pub(crate) fn number_of_months_in_range(year_range: &RangeInclusive<i32>) -> usize {
    ((*year_range.end() - *year_range.start() + 1).max(0) * 12) as usize
}

/// What a day does with the arrows and Tab, as Compose's `dayOnKeyEvent`: the arrows go along the
/// days, past a month's ends to the next one's; Tab leaves the month.
fn day_on_key_event(
    is_rtl: bool,
    is_first_day: bool,
    is_last_day: bool,
    state: LazyListState,
    coroutine_scope: CoroutineScope,
    focus_manager: Option<FocusManager>,
    on_return_focus: Callback,
) -> Modifier {
    let Some(focus) = focus_manager else { return Modifier };
    Modifier.on_key_event(move |event: KeyEvent| {
        let (forward, backward) = if is_rtl {
            (Key::DirectionLeft, Key::DirectionRight)
        } else {
            (Key::DirectionRight, Key::DirectionLeft)
        };
        let down = event.kind == KeyEventType::KeyDown;
        let is_forward = down && event.key == forward;
        let is_backward = down && event.key == backward;
        if is_first_day {
            if is_shift_tab(&event) {
                on_return_focus();
                return true;
            }
            if state.is_scroll_in_progress() {
                return true;
            }
            if is_backward {
                go_to_month(-1, state, focus.clone(), FocusDirection::Previous, coroutine_scope);
                return true;
            }
        } else if is_last_day {
            if is_tab(&event) {
                focus.move_focus(FocusDirection::Down);
                return true;
            }
            if state.is_scroll_in_progress() {
                return true;
            }
            if is_forward {
                go_to_month(1, state, focus.clone(), FocusDirection::Next, coroutine_scope);
                return true;
            }
        }
        if is_forward {
            focus.move_focus(FocusDirection::Next);
            true
        } else if is_backward {
            focus.move_focus(FocusDirection::Previous);
            true
        } else {
            false
        }
    })
}

fn go_to_month(
    delta: i32,
    state: LazyListState,
    focus_manager: FocusManager,
    direction: FocusDirection,
    coroutine_scope: CoroutineScope,
) {
    coroutine_scope.launch(async move {
        let index = state.first_visible_item_index() as i32 + delta;
        if index >= 0 && state.animate_scroll_to_item(index as usize, 0).await.is_ok() {
            focus_manager.move_focus(direction);
        }
    });
}

/// As `KeyEvent.isTab`.
pub(crate) fn is_tab(event: &KeyEvent) -> bool {
    !event.modifiers.shift && event.kind == KeyEventType::KeyDown && event.key == Key::Tab
}

/// As `KeyEvent.isShiftTab`.
pub(crate) fn is_shift_tab(event: &KeyEvent) -> bool {
    event.modifiers.shift && event.kind == KeyEventType::KeyDown && event.key == Key::Tab
}

fn first_enabled_day(month: &CalendarMonth, dates: &dyn SelectableDates) -> i32 {
    let mut first = month.days_from_start_of_week_to_first_of_month;
    let last = first + month.number_of_days - 1;
    if dates.is_selectable_year(month.year) {
        let mut day = 0;
        while first <= last
            && !dates
                .is_selectable_date(month.start_utc_time_millis + day * MILLISECONDS_IN_24_HOURS)
        {
            day += 1;
            first += 1;
        }
    }
    first
}

fn last_enabled_day(month: &CalendarMonth, dates: &dyn SelectableDates) -> i32 {
    let first = month.days_from_start_of_week_to_first_of_month;
    let mut last = first + month.number_of_days - 1;
    if dates.is_selectable_year(month.year) {
        let mut day = 0;
        while last >= first
            && !dates
                .is_selectable_date(month.end_utc_time_millis() - day * MILLISECONDS_IN_24_HOURS)
        {
            day += 1;
            last -= 1;
        }
    }
    last
}

fn day_content_description(
    range_selection: bool,
    is_today: bool,
    is_start_date: bool,
    is_end_date: bool,
    is_in_range: bool,
) -> Option<String> {
    let mut description = String::new();
    if range_selection {
        if is_start_date {
            description.push_str("Start date");
        } else if is_end_date {
            description.push_str("End date");
        } else if is_in_range {
            description.push_str("In range");
        }
    }
    if is_today {
        if !description.is_empty() {
            description.push_str(", ");
        }
        description.push_str("Today");
    }
    (!description.is_empty()).then_some(description)
}

/// As Compose's `Day`.
#[composable]
#[allow(clippy::too_many_arguments)]
fn Day(
    text: String,
    modifier: Modifier,
    selected: bool,
    on_click: impl Fn() + 'static,
    animate_checked: bool,
    enabled: bool,
    today: bool,
    in_range: bool,
    description: String,
    colors: DatePickerColors,
) {
    let spec = MaterialTheme::motion_scheme().default_effects_spec();
    let container_target = colors.day_container_color_target(selected, enabled);
    let container_color = if animate_checked {
        animate_color_as_state!(container_target, spec.clone()).get()
    } else {
        container_target
    };
    let content_target = colors.day_content_color_target(today, selected, in_range, enabled);
    let content_color =
        if in_range { content_target } else { animate_color_as_state!(content_target, spec).get() };
    SelectableSurface!(
        selected,
        on_click,
        modifier = modifier.semantics_with(true, move |config| {
            config.set_text(description.clone());
            config.set_role(Role::Button);
        }),
        enabled = enabled,
        shape = CIRCLE_SHAPE,
        color = container_color,
        border = (today && !selected)
            .then(|| BorderStroke::new(DATE_TODAY_OUTLINE_WIDTH, colors.today_date_border_color)),
        {
            Box!(
                modifier = Modifier.required_size((DATE_CONTAINER_SIZE, DATE_CONTAINER_SIZE)),
                content_alignment = Alignment::CENTER,
                {
                    Text!(
                        "{text}",
                        modifier = Modifier.clear_and_set_semantics(|_| {}),
                        color = content_color,
                        text_align = Some(TextAlign::Center)
                    );
                }
            );
        }
    );
}

/// As Compose's `YearPicker`.
#[composable]
#[allow(clippy::too_many_arguments)]
fn YearPicker(
    modifier: Modifier,
    displayed_month_millis: i64,
    on_year_selected: impl Fn(i32) + 'static,
    selectable_dates: Rc<dyn SelectableDates>,
    calendar_model: CalendarModel,
    year_range: YearRange,
    colors: DatePickerColors,
    current_year_focus_requester: FocusRequester,
    on_year_shift_tab_pressed: impl Fn() + 'static,
    on_year_tab_pressed: impl Fn() + 'static,
) {
    let current_year = calendar_model.get_month_of_date(&calendar_model.today()).year;
    let displayed_year = calendar_model.get_month_of_millis(displayed_month_millis).year;
    let first_year = *year_range.0.start();
    let count = year_range.0.clone().count();
    let focus_asked = remember(|| Rc::new(Cell::new(false)));
    let grid_state = remember_lazy_grid_state(
        (displayed_year - first_year - YEARS_IN_ROW as i32).max(0) as usize,
        0,
    );
    ProvideTextStyle!(MaterialTheme::typography().body_large.clone(), {
        LazyVerticalGrid!(
            GridCells::Fixed(YEARS_IN_ROW),
            modifier = modifier.clone().background(colors.container_color),
            state = grid_state,
            vertical_arrangement = Some(Arrangement::spaced_by(YEARS_VERTICAL_PADDING).into()),
            horizontal_arrangement = Arrangement::SPACE_EVENLY,
            |scope| {
                let calendar_model = calendar_model.clone();
                let on_year_selected = on_year_selected.clone();
                let selectable_dates = selectable_dates.clone();
                let on_year_shift_tab_pressed = on_year_shift_tab_pressed.clone();
                let on_year_tab_pressed = on_year_tab_pressed.clone();
                let focus_asked = focus_asked.clone();
                scope.items(count, move |index| {
                    let year = index as i32 + first_year;
                    let localized = to_local_string(year, 1, 40, calendar_model.locale());
                    let keys = Modifier.required_size((Dp(72.0), Dp(36.0))).on_key_event(
                        move |event: KeyEvent| {
                            if is_shift_tab(&event) {
                                on_year_shift_tab_pressed();
                                true
                            } else if is_tab(&event) {
                                on_year_tab_pressed();
                                true
                            } else {
                                false
                            }
                        },
                    );
                    Year!(
                        localized.clone(),
                        if year == displayed_year {
                            keys.focus_requester(current_year_focus_requester)
                        } else {
                            keys
                        },
                        year == displayed_year,
                        year == current_year,
                        move || on_year_selected(year),
                        selectable_dates.is_selectable_year(year),
                        format!("Navigate to year {localized}"),
                        colors
                    );
                    if year == displayed_year {
                        FocusDisplayedYear!(current_year_focus_requester, focus_asked.clone());
                    }
                });
            }
        );
    });
}

/// The displayed year takes the focus once the grid has composed it, the first time only: as
/// Compose's `LaunchedEffect` beside the grid, but here a grid entering with an animation composes
/// its items after that would run (docs/parity/foundation-layout-lazy.md).
#[composable]
fn FocusDisplayedYear(requester: FocusRequester, asked: Rc<Cell<bool>>) {
    LaunchedEffect((), async move {
        if !asked.replace(true) {
            requester.request_focus();
        }
    });
}

/// As Compose's `Year`.
#[composable]
#[allow(clippy::too_many_arguments)]
fn Year(
    text: String,
    modifier: Modifier,
    selected: bool,
    current_year: bool,
    on_click: impl Fn() + 'static,
    enabled: bool,
    description: String,
    colors: DatePickerColors,
) {
    let spec = MaterialTheme::motion_scheme().default_effects_spec();
    let container_color = animate_color_as_state!(
        colors.year_container_color_target(selected, enabled),
        spec.clone()
    )
    .get();
    let content_color = animate_color_as_state!(
        colors.year_content_color_target(current_year, selected, enabled),
        spec
    )
    .get();
    SelectableSurface!(
        selected,
        on_click,
        modifier = modifier.semantics_with(true, move |config| {
            config.set_text(description.clone());
            config.set_role(Role::Button);
        }),
        enabled = enabled,
        shape = CIRCLE_SHAPE,
        color = container_color,
        border = (current_year && !selected)
            .then(|| BorderStroke::new(DATE_TODAY_OUTLINE_WIDTH, colors.today_date_border_color)),
        {
            Box!(modifier = Modifier.fill_max_width(), content_alignment = Alignment::CENTER, {
                Text!(
                    "{text}",
                    modifier = Modifier.clear_and_set_semantics(|_| {}),
                    color = content_color,
                    text_align = Some(TextAlign::Center)
                );
            });
        }
    );
}

/// As Compose's `MonthsNavigation`.
#[composable]
#[allow(clippy::too_many_arguments)]
fn MonthsNavigation(
    modifier: Modifier,
    next_available: bool,
    previous_available: bool,
    year_picker_visible: bool,
    year_picker_text: String,
    next_button_modifier: Modifier,
    on_next_clicked: impl Fn() + 'static,
    on_previous_clicked: impl Fn() + 'static,
    on_year_picker_button_clicked: impl Fn() + 'static,
    on_year_picker_button_tab_pressed: impl Fn() + 'static,
    year_selection_button_focus_requester: FocusRequester,
    colors: DatePickerColors,
) {
    let navigation_color = colors.navigation_content_color;
    Row!(
        modifier = modifier.fill_max_width().required_height(MONTH_YEAR_HEIGHT),
        horizontal_arrangement = if year_picker_visible {
            Arrangement::START
        } else {
            Arrangement::SPACE_BETWEEN.into()
        },
        vertical_alignment = Alignment::CENTER_VERTICALLY,
        {
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(navigation_color), || {
                YearPickerMenuButton!(
                    on_year_picker_button_clicked,
                    year_picker_visible,
                    Modifier.focus_requester(year_selection_button_focus_requester).on_key_event(
                        move |event: KeyEvent| {
                            if year_picker_visible && is_tab(&event) {
                                on_year_picker_button_tab_pressed();
                                true
                            } else {
                                false
                            }
                        }
                    ),
                    {
                        let description = year_picker_text.clone();
                        Text!(
                            "{year_picker_text}",
                            modifier = Modifier.semantics(move |config| {
                                config.set_live_region(LiveRegionMode::Polite);
                                config.set_content_description(description.clone());
                            }),
                            color = navigation_color
                        );
                    }
                );
                if !year_picker_visible {
                    Row!({
                        IconButtonWithTooltip!(
                            on_previous_clicked,
                            icons::filled::keyboard_arrow_left(),
                            "Change to previous month",
                            Modifier,
                            previous_available
                        );
                        IconButtonWithTooltip!(
                            on_next_clicked,
                            icons::filled::keyboard_arrow_right(),
                            "Change to next month",
                            next_button_modifier.clone(),
                            next_available
                        );
                    });
                }
            });
        }
    );
}

/// As Compose's `YearPickerMenuButton`.
#[composable]
fn YearPickerMenuButton(
    on_click: impl Fn() + 'static,
    expanded: bool,
    modifier: Modifier,
    content: impl Fn() + 'static,
) {
    TextButton!(
        on_click,
        modifier = modifier,
        shape = CIRCLE_SHAPE,
        colors = crate::ButtonColors {
            content_color: LOCAL_CONTENT_COLOR.current(),
            ..ButtonDefaults::text_button_colors()
        },
        {
            content();
            Spacer!(modifier = Modifier.size(ButtonDefaults::ICON_SPACING));
            Icon!(
                icons::filled::arrow_drop_down(),
                Some(if expanded {
                    "Activate to change month and date"
                } else {
                    "Activate to change the year"
                }),
                modifier = Modifier.rotate(if expanded { 180.0 } else { 0.0 })
            );
        }
    );
}

/// As Compose's `IconButtonWithTooltip`.
#[composable]
fn IconButtonWithTooltip(
    on_click: impl Fn() + 'static,
    icon: compose_ui::ImageVector,
    content_description: &'static str,
    modifier: Modifier,
    enabled: bool,
) {
    TooltipBox!(
        TooltipDefaults::remember_tooltip_position_provider(TooltipAnchorPosition::Above),
        |scope| {
            PlainTooltip!(scope = Some(scope), {
                Text!("{content_description}");
            });
        },
        remember_tooltip_state!(),
        {
            IconButton!(on_click, modifier = modifier, enabled = enabled, {
                Icon!(icon, Some(content_description));
            });
        }
    );
}

/// A dialog of a date picker, as Compose's `DatePickerDialog`: the content over the buttons, which
/// it doesn't push out of view. Tab goes through the buttons as they read, the confirm one last,
/// though the flow row places it first.
#[composable]
pub fn DatePickerDialog(
    on_dismiss_request: impl Fn() + 'static,
    confirm_button: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] dismiss_button: Option<impl Fn() + 'static>,
    #[default(crate::DatePickerDefaults::shape())] shape: Shape,
    #[default(crate::DatePickerDefaults::TONAL_ELEVATION)] tonal_elevation: Dp,
    #[default(crate::DatePickerDefaults::colors())] colors: DatePickerColors,
    #[default(crate::window_default_dialog_properties())] properties: DialogProperties,
    content: impl Fn(ColumnScope) + 'static,
) {
    BasicAlertDialog!(
        on_dismiss_request,
        modifier = modifier.wrap_content_height(),
        properties = properties,
        {
            Surface!(
                modifier =
                    Modifier.required_width(CONTAINER_WIDTH).height_in(Dp::UNSPECIFIED, Dp(568.0)),
                shape = shape,
                color = colors.container_color,
                tonal_elevation = tonal_elevation,
                {
                    Column!(vertical_arrangement = Arrangement::SPACE_BETWEEN, |scope| {
                        // Not filling: the dialog follows a content that gets smaller.
                        Box!(modifier = Modifier.weight_with(scope, 1.0, false), {
                            content(scope);
                        });
                        Box!(
                            modifier = Modifier.align(scope, Alignment::END).padding((
                                Dp(0.0),
                                Dp(0.0),
                                Dp(6.0),
                                Dp(8.0)
                            )),
                            {
                                dialog_buttons(confirm_button.clone(), dismiss_button.clone());
                            }
                        );
                    });
                }
            );
        }
    );
}

/// The dialog's buttons, in the dialog's action colors and text style.
fn dialog_buttons(confirm_button: Callback, dismiss_button: Option<Callback>) {
    let focus = LOCAL_FOCUS_MANAGER.current();
    let is_rtl = LOCAL_LAYOUT_DIRECTION.current() == LayoutDirection::Rtl;
    let (to_confirm, to_dismiss) = if is_rtl {
        (FocusDirection::Left, FocusDirection::Right)
    } else {
        (FocusDirection::Right, FocusDirection::Left)
    };
    let has_dismiss = dismiss_button.is_some();
    let style = MaterialTheme::typography().label_large.clone();
    let (confirm_focus, dismiss_focus) = (focus.clone(), focus);
    let on_confirm_key = move |event: KeyEvent| {
        if is_tab(&event) {
            confirm_focus.move_focus(FocusDirection::Next);
            if has_dismiss {
                // On the dismiss button, placed after: on again.
                confirm_focus.move_focus(FocusDirection::Next);
            }
            true
        } else if is_shift_tab(&event) && has_dismiss {
            confirm_focus.move_focus(to_dismiss);
            true
        } else {
            false
        }
    };
    let on_dismiss_key = move |event: KeyEvent| {
        if is_tab(&event) {
            if !dismiss_focus.move_focus(to_confirm) {
                // The confirm button isn't enabled: on along.
                dismiss_focus.move_focus(FocusDirection::Next);
            }
            true
        } else if is_shift_tab(&event) {
            // By the confirm button, back to the month's last day.
            dismiss_focus.move_focus(to_confirm);
            dismiss_focus.move_focus(FocusDirection::Previous);
            true
        } else {
            false
        }
    };
    provide_content_color_text_style(MaterialTheme::color_scheme().primary, style, move || {
        let (confirm_button, dismiss_button) = (confirm_button.clone(), dismiss_button.clone());
        let (on_confirm_key, on_dismiss_key) = (on_confirm_key.clone(), on_dismiss_key.clone());
        alert_dialog_flow_row(
            Dp(8.0),
            Dp(8.0),
            compose_runtime::callback(move || {
                let confirm_button = confirm_button.clone();
                let dismiss_button = dismiss_button.clone();
                Box!(modifier = Modifier.on_key_event(on_confirm_key.clone()), {
                    confirm_button();
                });
                Box!(modifier = Modifier.on_key_event(on_dismiss_key.clone()), {
                    if let Some(dismiss_button) = &dismiss_button {
                        dismiss_button();
                    }
                });
            }),
        );
    });
}

pub(crate) const RECOMMENDED_SIZE_FOR_ACCESSIBILITY: Dp = Dp(48.0);
pub(crate) const MONTH_YEAR_HEIGHT: Dp = Dp(56.0);
pub(crate) const DATE_PICKER_HORIZONTAL_PADDING: Dp = Dp(12.0);
const CONTAINER_WIDTH: Dp = Dp(360.0);
const DATE_CONTAINER_SIZE: Dp = Dp(40.0);
const DATE_TODAY_OUTLINE_WIDTH: Dp = Dp(1.0);
const YEARS_VERTICAL_PADDING: Dp = Dp(16.0);
pub(crate) const MAX_CALENDAR_ROWS: i32 = 6;
const YEARS_IN_ROW: usize = 3;
