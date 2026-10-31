//! The date input, as Compose's `DateInput`: a field of digits showing the locale's delimiters.
//! Over a field of a `String` until `TextFieldState` is there (D1562): what Compose's input and
//! output transformations do, the change's handler and a visual transformation do.

use std::cell::Cell;
use std::ops::RangeInclusive;
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::{
    ImeAction, KeyboardOptions, KeyboardType, OffsetMapping, TransformedText, VisualTransform,
    VisualTransformation,
};
use compose_runtime::{
    LaunchedEffect, composable, delay, mutable_state_of, remember_keyed, remember_saveable,
};
use compose_ui::{Dp, FocusRequester, Locale, Modifier};

use crate::calendar_model::{
    CalendarDate, CalendarModel, DateInputFormat, is_date_digit, to_local_string,
};
use crate::components::Text;
use crate::date_picker::{DatePickerColors, DatePickerFormatter, SelectableDates, YearRange};
use crate::text_field::OutlinedTextField;

/// Which date a field holds, as Compose's `InputIdentifier`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum InputIdentifier {
    Single,
    #[allow(dead_code)]
    Start,
    #[allow(dead_code)]
    End,
}

impl compose_runtime::ComposeArg for InputIdentifier {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Says what's wrong with a typed date, as Compose's `DateInputValidator`.
pub(crate) struct DateInputValidator {
    year_range: RangeInclusive<i32>,
    selectable_dates: Rc<dyn SelectableDates>,
    date_input_format: DateInputFormat,
    date_formatter: Rc<dyn DatePickerFormatter>,
    /// The start date, for an end date's checks.
    pub(crate) current_start_date_millis: Cell<Option<i64>>,
    /// The end date, for a start date's checks.
    pub(crate) current_end_date_millis: Cell<Option<i64>>,
}

impl DateInputValidator {
    pub(crate) fn new(
        year_range: RangeInclusive<i32>,
        selectable_dates: Rc<dyn SelectableDates>,
        date_input_format: DateInputFormat,
        date_formatter: Rc<dyn DatePickerFormatter>,
    ) -> DateInputValidator {
        DateInputValidator {
            year_range,
            selectable_dates,
            date_input_format,
            date_formatter,
            current_start_date_millis: Cell::new(None),
            current_end_date_millis: Cell::new(None),
        }
    }

    /// The error, empty when there's none, as `validate`.
    pub(crate) fn validate(
        &self,
        date: Option<CalendarDate>,
        input_identifier: InputIdentifier,
        locale: &Locale,
    ) -> String {
        let Some(date) = date else {
            return format!(
                "Date does not match expected pattern: {}",
                self.date_input_format.pattern_with_delimiters.to_uppercase()
            );
        };
        if !self.year_range.contains(&date.year) {
            return format!(
                "Date out of expected year range {} - {}",
                to_local_string(*self.year_range.start(), 1, 40, locale),
                to_local_string(*self.year_range.end(), 1, 40, locale),
            );
        }
        if !self.selectable_dates.is_selectable_year(date.year)
            || !self.selectable_dates.is_selectable_date(date.utc_time_millis)
        {
            let formatted = self
                .date_formatter
                .format_date(Some(date.utc_time_millis), locale, false)
                .unwrap_or_default();
            return format!("Date not allowed: {formatted}");
        }
        let after_end = input_identifier == InputIdentifier::Start
            && date.utc_time_millis > self.current_end_date_millis.get().unwrap_or(i64::MAX);
        let before_start = input_identifier == InputIdentifier::End
            && date.utc_time_millis < self.current_start_date_millis.get().unwrap_or(i64::MIN);
        if after_end || before_start {
            return "Invalid date range input".to_string();
        }
        String::new()
    }
}

/// As Compose's `DateInputContent`.
#[composable]
#[allow(clippy::too_many_arguments)]
pub(crate) fn DateInputContent(
    selected_date_millis: Option<i64>,
    on_date_selection_change: impl Fn(Option<i64>) + 'static,
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
    validator.current_start_date_millis.set(selected_date_millis);
    let pattern = date_input_format.pattern_with_delimiters.to_uppercase();
    let label_pattern = pattern.clone();
    let locale = calendar_model.locale().clone();
    DateInputTextField(
        Modifier.fill_max_width().padding((Dp(24.0), Dp(10.0), Dp(24.0), Dp(0.0))),
        selected_date_millis,
        on_date_selection_change,
        calendar_model,
        Some(compose_runtime::callback(move || {
            let description = format!("Date, {label_pattern}");
            Text!(
                "Date",
                modifier = Modifier
                    .semantics(move |config| config.set_content_description(description.clone()))
            );
        })),
        Some(compose_runtime::callback(move || {
            Text!("{pattern}", modifier = Modifier.clear_and_set_semantics(|_| {}));
        })),
        InputIdentifier::Single,
        validator,
        date_input_format,
        locale,
        colors,
        focus_requester,
    );
}

/// As Compose's `DateInputTextField`.
#[composable]
#[allow(clippy::too_many_arguments)]
pub(crate) fn DateInputTextField(
    modifier: Modifier,
    initial_date_millis: Option<i64>,
    on_date_selection_change: impl Fn(Option<i64>) + 'static,
    calendar_model: CalendarModel,
    label: Option<impl Fn() + 'static>,
    placeholder: Option<impl Fn() + 'static>,
    input_identifier: InputIdentifier,
    date_input_validator: Rc<DateInputValidator>,
    date_input_format: DateInputFormat,
    locale: Locale,
    colors: DatePickerColors,
    focus_requester: Option<FocusRequester>,
) {
    let without_delimiters = date_input_format.pattern_without_delimiters();
    let format_date = {
        let (model, pattern, locale) =
            (calendar_model.clone(), without_delimiters.clone(), locale.clone());
        move |millis: i64| model.format_with_pattern(millis, &pattern, &locale)
    };
    let text = {
        let format_date = format_date.clone();
        remember_saveable(move || {
            mutable_state_of(initial_date_millis.map_or_else(String::new, &format_date))
        })
    };
    let pattern_length = without_delimiters.chars().count();
    let parse = {
        let (model, pattern, locale) =
            (calendar_model.clone(), without_delimiters.clone(), locale.clone());
        move |text: &str| model.parse(text, &pattern, &locale)
    };

    let current = text.get();
    let error_text = if current.trim().chars().count() == pattern_length {
        date_input_validator.validate(parse(current.trim()), input_identifier, &locale)
    } else {
        String::new()
    };
    let is_error = !error_text.trim().is_empty();

    // As Compose's `DateInputTransformation`: what isn't digits, or is too long, doesn't go in.
    let on_value_change = {
        let (validator, locale) = (date_input_validator.clone(), locale.clone());
        compose_runtime::callback(move |new_text: String| {
            if new_text.chars().count() > pattern_length
                || !new_text.chars().all(|character| is_date_digit(character, &locale))
            {
                return;
            }
            text.set(new_text.clone());
            if new_text.chars().count() < pattern_length {
                on_date_selection_change(None);
            } else {
                let parsed = parse(&new_text);
                let error = validator.validate(parsed, input_identifier, &locale);
                on_date_selection_change(if error.is_empty() {
                    parsed.map(|date| date.utc_time_millis)
                } else {
                    None
                });
            }
        })
    };

    let transformation = remember_keyed(date_input_format.clone(), || {
        VisualTransformation::new(DateOutputTransformation::new(&date_input_format))
    });

    // The room an error's text takes comes off the padding: the height doesn't change. The
    // supporting text's own padding is 4 dp above it.
    let bottom_padding = if is_error { Dp(16.0 - 4.0) } else { Dp(16.0) };
    let requester_modifier = match focus_requester {
        Some(requester) => Modifier.focus_requester(requester),
        None => Modifier,
    };
    let error_for_semantics = error_text.clone();
    OutlinedTextField!(
        current,
        on_value_change,
        modifier = modifier
            .padding((Dp(0.0), Dp(0.0), Dp(0.0), bottom_padding))
            .semantics(move |config| {
                if is_error {
                    config.error(error_for_semantics.clone());
                }
            })
            .then(requester_modifier),
        label = label,
        placeholder = placeholder,
        supporting_text = Some(move || {
            if is_error {
                Text!("{error_text}");
            }
        }),
        is_error = is_error,
        visual_transformation = transformation,
        keyboard_options = KeyboardOptions {
            auto_correct_enabled: false,
            keyboard_type: KeyboardType::Number,
            ime_action: ImeAction::Done,
            ..KeyboardOptions::DEFAULT
        },
        single_line = true,
        colors = colors.date_text_field_colors
    );

    // In a dialog the transition goes first, as Compose's delay of `DurationMedium2`.
    LaunchedEffect((), async move {
        if let Some(requester) = focus_requester {
            delay(Duration::from_millis(300)).await;
            requester.request_focus();
        }
    });

    LaunchedEffect(initial_date_millis, async move {
        if let Some(millis) = initial_date_millis {
            let date_text = format_date(millis);
            if text.get() != date_text {
                text.set(date_text);
            }
        }
    });
}

/// Shows a date's digits with its delimiters as soon as what's before them is typed, as Compose's
/// `DateOutputTransformation`.
struct DateOutputTransformation {
    /// The characters before the first and the second delimiters, in the pattern with them.
    first_delimiter: usize,
    second_delimiter: usize,
    delimiter: char,
}

impl DateOutputTransformation {
    fn new(format: &DateInputFormat) -> DateOutputTransformation {
        let pattern = &format.pattern_with_delimiters;
        DateOutputTransformation {
            first_delimiter: pattern.find(format.delimiter).unwrap_or(usize::MAX),
            second_delimiter: pattern.rfind(format.delimiter).unwrap_or(usize::MAX),
            delimiter: format.delimiter,
        }
    }
}

impl VisualTransform for DateOutputTransformation {
    fn filter(&self, text: &str) -> TransformedText {
        let mut shown = String::with_capacity(text.len() + 2);
        // Byte offsets in the shown text after which a byte of the text has gone by a delimiter.
        let mut delimiters = Vec::new();
        let mut count = 0;
        let mut boundary = |shown: &mut String, count: usize, original: usize| {
            let at_first = count == self.first_delimiter;
            let at_second =
                self.second_delimiter > self.first_delimiter && count + 1 == self.second_delimiter;
            if at_first || at_second {
                shown.push(self.delimiter);
                delimiters.push(original);
            }
        };
        boundary(&mut shown, count, 0);
        for (index, character) in text.char_indices() {
            shown.push(character);
            count += 1;
            boundary(&mut shown, count, index + character.len_utf8());
        }
        let length = self.delimiter.len_utf8();
        TransformedText { text: shown, offset_mapping: Rc::new(DateOffsets { delimiters, length }) }
    }
}

/// The offsets in a text the delimiters come after, as the transformation's mapping.
struct DateOffsets {
    delimiters: Vec<usize>,
    length: usize,
}

impl OffsetMapping for DateOffsets {
    fn original_to_transformed(&self, offset: usize) -> usize {
        let before = self.delimiters.iter().filter(|at| **at <= offset).count();
        offset + before * self.length
    }

    fn transformed_to_original(&self, offset: usize) -> usize {
        let mut shown_before = 0;
        for (index, at) in self.delimiters.iter().enumerate() {
            let shown_at = at + index * self.length;
            if offset <= shown_at {
                break;
            }
            shown_before = (offset - shown_at).min(self.length) + index * self.length;
        }
        offset - shown_before
    }
}
