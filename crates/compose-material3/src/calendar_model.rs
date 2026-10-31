//! The calendar under the date pickers, as Compose's internal `CalendarModel`: dates and months
//! in UTC, what a locale calls them and how it writes them.
//!
//! The calendar's arithmetic is proleptic Gregorian, as java.time's. What's the locale's comes
//! from the small tables at the end of this file: the languages Compose's tests read, and English
//! for the others (D1562). Its one `unsafe` is the local zone, `localtime_r`. The platforms' real data, Android's ICU and a desktop crate, are
//! `calendar-model (platforms)` in docs/parity/material3.md.

use std::cmp::Ordering;
use std::ops::RangeInclusive;
use std::time::{SystemTime, UNIX_EPOCH};

use compose_ui::Locale;

/// The days in a week, as `DaysInWeek`.
pub const DAYS_IN_WEEK: i32 = 7;
/// The milliseconds in a UTC day, as `MillisecondsIn24Hours`.
pub const MILLISECONDS_IN_24_HOURS: i64 = 86_400_000;

/// A day, as Compose's `CalendarDate`: `utc_time_millis` is its start, and orders dates.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct CalendarDate {
    pub year: i32,
    pub month: i32,
    pub day_of_month: i32,
    pub utc_time_millis: i64,
}

impl Ord for CalendarDate {
    fn cmp(&self, other: &CalendarDate) -> Ordering {
        self.utc_time_millis.cmp(&other.utc_time_millis)
    }
}

impl PartialOrd for CalendarDate {
    fn partial_cmp(&self, other: &CalendarDate) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl CalendarDate {
    /// As `format(calendarModel, skeleton)`.
    pub fn format(&self, model: &CalendarModel, skeleton: &str) -> String {
        model.format_date_with_skeleton(self, skeleton)
    }
}

/// A month, as Compose's `CalendarMonth`: `days_from_start_of_week_to_first_of_month` is the empty
/// cells before its first day, in a week starting on the model's first day.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct CalendarMonth {
    pub year: i32,
    pub month: i32,
    pub number_of_days: i32,
    pub days_from_start_of_week_to_first_of_month: i32,
    pub start_utc_time_millis: i64,
}

impl CalendarMonth {
    /// The last millisecond of the month, as `endUtcTimeMillis`.
    pub fn end_utc_time_millis(&self) -> i64 {
        self.start_utc_time_millis
            .saturating_add(i64::from(self.number_of_days) * MILLISECONDS_IN_24_HOURS - 1)
    }

    /// Its place among the months of `years`, January of the first being 0, as `indexIn`.
    pub fn index_in(&self, years: &RangeInclusive<i32>) -> i32 {
        let months = (i64::from(self.year) - i64::from(*years.start())) * 12;
        (months + i64::from(self.month) - 1).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }

    /// As `format(calendarModel, skeleton)`.
    pub fn format(&self, model: &CalendarModel, skeleton: &str) -> String {
        model.format_month_with_skeleton(self, skeleton)
    }
}

/// How a date is typed, as Compose's `DateInputFormat`.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct DateInputFormat {
    pub pattern_with_delimiters: String,
    pub delimiter: char,
}

impl DateInputFormat {
    /// As `patternWithoutDelimiters`.
    pub fn pattern_without_delimiters(&self) -> String {
        self.pattern_with_delimiters.replace(self.delimiter, "")
    }
}

/// The calendar in a locale, as Compose's `CalendarModel`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CalendarModel {
    locale: Locale,
    first_day_of_week: i32,
}

impl CalendarModel {
    /// As `createCalendarModel(locale)`.
    pub fn new(locale: Locale) -> CalendarModel {
        let first_day_of_week = first_day_of_week(&locale);
        CalendarModel { locale, first_day_of_week }
    }

    /// As `locale`.
    pub fn locale(&self) -> &Locale {
        &self.locale
    }

    /// Today in the local zone, as `today`: a UTC day holding the local date. In UTC where the zone
    /// isn't known (see [`local_offset_seconds`]).
    pub fn today(&self) -> CalendarDate {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
        self.get_canonical_date(now + local_offset_seconds(now / 1000) * 1000)
    }

    /// The week's first day, 1 being Monday, as `firstDayOfWeek`.
    pub fn first_day_of_week(&self) -> i32 {
        self.first_day_of_week
    }

    /// The weekdays' full and narrow names, Monday first, as `weekdayNames`.
    pub fn weekday_names(&self) -> Vec<(&'static str, &'static str)> {
        let data = language_data(&self.locale);
        (0..7).map(|day| (data.weekdays[day], data.weekdays_narrow[day])).collect()
    }

    /// The input format of `locale`'s short dates, as `getDateInputFormat`.
    pub fn get_date_input_format(&self) -> DateInputFormat {
        date_pattern_as_input_format(short_date_pattern(&self.locale))
    }

    /// The date `millis` falls in, as `getCanonicalDate`.
    pub fn get_canonical_date(&self, millis: i64) -> CalendarDate {
        let days = millis.div_euclid(MILLISECONDS_IN_24_HOURS);
        let (year, month, day_of_month) = civil_from_days(days);
        // The start of `i64::MIN`'s day is before what millis can say: Java's throws.
        let utc_time_millis = days.saturating_mul(MILLISECONDS_IN_24_HOURS);
        CalendarDate { year, month, day_of_month, utc_time_millis }
    }

    /// The month `millis` falls in, as `getMonth(timeInMillis)`.
    pub fn get_month_of_millis(&self, millis: i64) -> CalendarMonth {
        let (year, month, _) = civil_from_days(millis.div_euclid(MILLISECONDS_IN_24_HOURS));
        self.get_month(year, month)
    }

    /// The month of `date`, as `getMonth(date)`.
    pub fn get_month_of_date(&self, date: &CalendarDate) -> CalendarMonth {
        self.get_month(date.year, date.month)
    }

    /// The `month` (1 to 12) of `year`, as `getMonth(year, month)`.
    pub fn get_month(&self, year: i32, month: i32) -> CalendarMonth {
        let days = days_from_civil(year, month, 1);
        let days_from_start =
            (day_of_week_of_days(days) - self.first_day_of_week).rem_euclid(DAYS_IN_WEEK);
        CalendarMonth {
            year,
            month,
            number_of_days: length_of_month(year, month),
            days_from_start_of_week_to_first_of_month: days_from_start,
            start_utc_time_millis: days.saturating_mul(MILLISECONDS_IN_24_HOURS),
        }
    }

    /// The day of the week of `date`, 1 being Monday, as `getDayOfWeek`.
    pub fn get_day_of_week(&self, date: &CalendarDate) -> i32 {
        day_of_week_of_days(days_from_civil(date.year, date.month, date.day_of_month))
    }

    /// `count` months after `from`, `from` for a count that isn't positive, as `plusMonths`.
    pub fn plus_months(&self, from: &CalendarMonth, count: i32) -> CalendarMonth {
        if count <= 0 {
            return *from;
        }
        let months = i64::from(from.year) * 12 + i64::from(from.month) - 1 + i64::from(count);
        self.get_month(months.div_euclid(12) as i32, months.rem_euclid(12) as i32 + 1)
    }

    /// `count` months before `from`, `from` for a count that isn't positive, as `minusMonths`.
    pub fn minus_months(&self, from: &CalendarMonth, count: i32) -> CalendarMonth {
        if count <= 0 {
            return *from;
        }
        let months = i64::from(from.year) * 12 + i64::from(from.month) - 1 - i64::from(count);
        self.get_month(months.div_euclid(12) as i32, months.rem_euclid(12) as i32 + 1)
    }

    /// As `formatWithSkeleton(date, skeleton)`, in the model's locale.
    pub fn format_date_with_skeleton(&self, date: &CalendarDate, skeleton: &str) -> String {
        format_with_skeleton(date.utc_time_millis, skeleton, &self.locale)
    }

    /// As `formatWithSkeleton(month, skeleton)`, in the model's locale.
    pub fn format_month_with_skeleton(&self, month: &CalendarMonth, skeleton: &str) -> String {
        format_with_skeleton(month.start_utc_time_millis, skeleton, &self.locale)
    }

    /// `millis` written by a pattern's letters, as `formatWithPattern`.
    pub fn format_with_pattern(&self, millis: i64, pattern: &str, locale: &Locale) -> String {
        format_with_pattern(millis, pattern, locale)
    }

    /// The date `date` reads as by a pattern, or `None`, as `parse`. A day past its month's end is
    /// the month's last, as java.time's smart resolving, which Compose's date input goes through.
    pub fn parse(&self, date: &str, pattern: &str, locale: &Locale) -> Option<CalendarDate> {
        let zero = zero_digit(locale);
        let data = language_data(locale);
        let mut text: &str = date;
        let (mut year, mut month, mut day) = (None, None, None);
        let fields = pattern_fields(pattern);
        for (index, field) in fields.iter().enumerate() {
            match field {
                PatternField::Literal(literal) => text = text.strip_prefix(literal.as_str())?,
                &PatternField::Letter(letter, width) => match letter {
                    'y' | 'Y' => {
                        // As java.time's adjacent values: the numeric fields right after it keep
                        // their digits, so `yyyyMMdd` reads.
                        let reserved: usize = fields[index + 1..]
                            .iter()
                            .map_while(|field| match *field {
                                PatternField::Letter('y' | 'Y' | 'd', width) => Some(width),
                                PatternField::Letter('M' | 'L', width) if width < 3 => Some(width),
                                _ => None,
                            })
                            .sum();
                        let digits = text.chars().take_while(|c| is_digit(*c, zero)).count();
                        let max = if width == 2 { 2 } else { digits.saturating_sub(reserved) };
                        let (value, rest) = parse_number(text, zero, width, max)?;
                        // Two letters are a year of 2000 to 2099, as java.time's `yy`.
                        year = Some(if width == 2 { 2000 + value } else { value });
                        text = rest;
                    }
                    'M' | 'L' if width >= 3 => {
                        let names = if width == 3 { &data.months_short } else { &data.months };
                        let index = names.iter().position(|name| text.starts_with(name))?;
                        month = Some(index as i64 + 1);
                        text = &text[names[index].len()..];
                    }
                    'M' | 'L' => {
                        let (value, rest) = parse_number(text, zero, width, fixed_max(width))?;
                        month = Some(value);
                        text = rest;
                    }
                    'd' => {
                        let (value, rest) = parse_number(text, zero, width, fixed_max(width))?;
                        day = Some(value);
                        text = rest;
                    }
                    _ => return None,
                },
            }
        }
        if !text.is_empty() {
            return None;
        }
        // `y` is the year of the era: from 1, as java.time's, and no further than it goes.
        let year = i32::try_from(year?).ok().filter(|year| (1..=999_999_999).contains(year))?;
        let month = i32::try_from(month?).ok()?;
        let day = i32::try_from(day?).ok()?;
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        let day = day.min(length_of_month(year, month));
        Some(CalendarDate {
            year,
            month,
            day_of_month: day,
            utc_time_millis: days_from_civil(year, month, day)
                .checked_mul(MILLISECONDS_IN_24_HOURS)?,
        })
    }
}

impl compose_runtime::ComposeArg for CalendarModel {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl compose_runtime::ComposeArg for DateInputFormat {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl compose_runtime::ComposeArg for CalendarMonth {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// One letter asks for at least a digit; two or more for exactly as many.
fn fixed_max(width: usize) -> usize {
    if width == 1 { usize::MAX } else { width }
}

fn is_digit(character: char, zero: char) -> bool {
    (character as u32).wrapping_sub(zero as u32) <= 9
}

/// Whether a date field takes this character: an ASCII digit or one of the locale's ten.
pub fn is_date_digit(character: char, locale: &Locale) -> bool {
    character.is_ascii_digit() || is_digit(character, zero_digit(locale))
}

/// The local zone's offset from UTC at `seconds` from the epoch, by `localtime_r`. On Android
/// that's bionic's reading of the system's zone, not yet seen on a phone (docs/parity/material3.md,
/// `calendar-model (platforms)`). 0 where there's no `localtime_r`, and under Miri, which has none.
#[cfg(all(unix, not(miri)))]
#[allow(unsafe_code)]
pub fn local_offset_seconds(seconds: i64) -> i64 {
    let time = seconds as libc::time_t;
    // SAFETY: an all-zero `tm` is valid (its one pointer, `tm_zone`, null); `localtime_r` reads
    // `time` and writes `local`, both alive across the call, and keeps neither.
    let (result, local) = unsafe {
        let mut local: libc::tm = std::mem::zeroed();
        let result = libc::localtime_r(&time, &mut local);
        (result, local)
    };
    if result.is_null() { 0 } else { i64::from(local.tm_gmtoff as i32) }
}

/// The local zone's offset from UTC: unknown here, so none.
#[cfg(not(all(unix, not(miri))))]
pub fn local_offset_seconds(_seconds: i64) -> i64 {
    0
}

/// A number in the locale's digits: at least `min` of them, at most `max`.
fn parse_number(text: &str, zero: char, min: usize, max: usize) -> Option<(i64, &str)> {
    let mut value: i64 = 0;
    let mut count = 0;
    let mut end = 0;
    for (index, character) in text.char_indices() {
        let digit = (character as u32).wrapping_sub(zero as u32);
        if digit > 9 || count == max {
            break;
        }
        value = value.checked_mul(10)?.checked_add(i64::from(digit))?;
        count += 1;
        end = index + character.len_utf8();
    }
    (count >= min).then(|| (value, &text[end..]))
}

/// The default locale of what's below, as `defaultLocale()`: the first of `LocalLocaleList`, and
/// `en-US` until the platforms' are read (docs/parity/ui-text-and-test.md, "locale").
pub fn default_locale() -> Locale {
    compose_ui::LOCAL_LOCALE_LIST.current().get(0).cloned().unwrap_or_else(|| Locale::new("en-US"))
}

/// `value` in `locale`'s digits, as `Int.toLocalString`: at least `min_digits`, at most
/// `max_digits` (the lowest).
pub fn to_local_string(
    value: i32,
    min_digits: usize,
    max_digits: usize,
    locale: &Locale,
) -> String {
    let negative = value < 0;
    let digits = value.unsigned_abs().to_string();
    let digits = &digits[digits.len().saturating_sub(max_digits)..];
    let mut result = String::with_capacity(digits.len() + 1);
    if negative {
        result.push('-');
    }
    for _ in digits.len()..min_digits {
        push_digit(&mut result, zero_digit(locale), 0);
    }
    for digit in digits.bytes() {
        push_digit(&mut result, zero_digit(locale), u32::from(digit - b'0'));
    }
    result
}

fn push_digit(result: &mut String, zero: char, digit: u32) {
    result.push(char::from_u32(zero as u32 + digit).unwrap_or('?'));
}

/// `millis` by a skeleton, whose order doesn't count: the locale's pattern for those fields,
/// capitalized as the locale does a date standing alone, as `formatWithSkeleton`.
pub fn format_with_skeleton(millis: i64, skeleton: &str, locale: &Locale) -> String {
    let data = language_data(locale);
    let formatted = format_with_pattern(millis, &best_pattern(skeleton, locale), locale);
    if !data.capitalized_alone {
        return formatted;
    }
    let mut characters = formatted.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => formatted,
    }
}

/// `millis` written by a pattern's letters (`d`, `M`, `L`, `y`, `Y` and `E`, `'` quoting), as
/// `formatWithPattern`. `Y` is written as `y`: its week year differs only around the new year, and
/// no pattern of the pickers has it.
pub fn format_with_pattern(millis: i64, pattern: &str, locale: &Locale) -> String {
    let zero = zero_digit(locale);
    let data = language_data(locale);
    let days = millis.div_euclid(MILLISECONDS_IN_24_HOURS);
    let (year, month, day) = civil_from_days(days);
    let month_index = (month - 1) as usize;
    let weekday_index = (day_of_week_of_days(days) - 1) as usize;
    let year_of_era = if year >= 1 { year } else { 1i32.saturating_sub(year) };
    let mut result = String::new();
    let number = |result: &mut String, value: i32, width: usize| {
        for digit in format!("{value:0width$}").bytes() {
            match digit {
                b'-' => result.push('-'),
                digit => push_digit(result, zero, u32::from(digit - b'0')),
            }
        }
    };
    for field in pattern_fields(pattern) {
        match field {
            PatternField::Literal(literal) => result.push_str(&literal),
            PatternField::Letter(letter, width) => match letter {
                // The year of the era, as java.time's `y`: year 0 is 1 BC.
                'y' | 'Y' if width == 2 => number(&mut result, year_of_era.rem_euclid(100), 2),
                'y' | 'Y' => number(&mut result, year_of_era, width),
                'M' | 'L' if width >= 4 => result.push_str(data.months[month_index]),
                'M' | 'L' if width == 3 => result.push_str(data.months_short[month_index]),
                'M' | 'L' => number(&mut result, month, width),
                'd' => number(&mut result, day, width),
                'E' if width == 4 => result.push_str(data.weekdays[weekday_index]),
                'E' if width >= 5 => result.push_str(data.weekdays_narrow[weekday_index]),
                'E' => result.push_str(data.weekdays_short[weekday_index]),
                other => result.extend(std::iter::repeat_n(other, width)),
            },
        }
    }
    result
}

enum PatternField {
    /// A letter and how many times it's there in a row.
    Letter(char, usize),
    Literal(String),
}

fn pattern_fields(pattern: &str) -> Vec<PatternField> {
    let mut fields = Vec::new();
    let mut literal = String::new();
    let mut characters = pattern.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\'' {
            if characters.peek() == Some(&'\'') {
                characters.next();
                literal.push('\'');
                continue;
            }
            for quoted in characters.by_ref() {
                if quoted == '\'' {
                    break;
                }
                literal.push(quoted);
            }
        } else if character.is_ascii_alphabetic() {
            let mut width = 1;
            while characters.next_if_eq(&character).is_some() {
                width += 1;
            }
            if !literal.is_empty() {
                fields.push(PatternField::Literal(std::mem::take(&mut literal)));
            }
            fields.push(PatternField::Letter(character, width));
        } else {
            literal.push(character);
        }
    }
    if !literal.is_empty() {
        fields.push(PatternField::Literal(literal));
    }
    fields
}

/// The locale's pattern for a skeleton's fields, its letters as wide as the skeleton's where
/// they're words.
fn best_pattern(skeleton: &str, locale: &Locale) -> String {
    let data = language_data(locale);
    let width = |letters: &[char]| skeleton.chars().filter(|c| letters.contains(c)).count();
    let (years, months, days, weekdays) =
        (width(&['y']), width(&['M', 'L']), width(&['d']), width(&['E']));
    let base = match (years > 0, months >= 3, days > 0, weekdays > 0) {
        (true, true, true, true) => data.year_month_weekday_day,
        (true, true, true, false) => data.year_month_day,
        (true, true, false, false) => data.year_month,
        _ if years > 0 && months > 0 && days > 0 && months < 3 => {
            // All numbers: the short date, with its whole year.
            return short_date_pattern(locale).replace("yy", "y");
        }
        // Fewer fields than a table has: its longest pattern without the others.
        _ if months != 1
            && months != 2
            && [months, days, weekdays].iter().filter(|width| **width > 0).count() >= 2
            && skeleton.chars().all(|c| matches!(c, 'M' | 'L' | 'd' | 'E')) =>
        {
            return some_fields_of(data.year_month_weekday_day, months, days, weekdays);
        }
        // Fields no table has: as the skeleton gives them.
        _ => {
            let parts: Vec<String> = pattern_fields(skeleton)
                .into_iter()
                .map(|field| match field {
                    PatternField::Letter(letter, width) => letter.to_string().repeat(width),
                    PatternField::Literal(literal) => format!("'{literal}'"),
                })
                .collect();
            return parts.join(" ");
        }
    };
    let mut pattern = String::new();
    for field in pattern_fields(base) {
        match field {
            PatternField::Letter('M', width) if width >= 3 => {
                pattern.push_str(&"M".repeat(months));
            }
            PatternField::Letter('E', _) => pattern.push_str(&"E".repeat(weekdays)),
            PatternField::Letter(letter, width) => {
                pattern.push_str(&letter.to_string().repeat(width));
            }
            PatternField::Literal(literal) => {
                pattern.push('\'');
                pattern.push_str(&literal.replace('\'', "''"));
                pattern.push('\'');
            }
        }
    }
    pattern
}

/// `pattern` with only the month, day and weekday asked for: what's between two of them stays, and
/// what ends a field in a language that writes one so (`日`, `일`).
fn some_fields_of(pattern: &str, months: usize, days: usize, weekdays: usize) -> String {
    let wanted = |letter: char| match letter {
        'M' => months > 0,
        'd' => days > 0,
        'E' => weekdays > 0,
        _ => false,
    };
    let fields = pattern_fields(pattern);
    let mut result = String::new();
    for (index, field) in fields.iter().enumerate() {
        match field {
            &PatternField::Letter(letter, width) if wanted(letter) => {
                let width = match letter {
                    'M' if width >= 3 => months,
                    'E' => weekdays,
                    _ => width,
                };
                result.push_str(&letter.to_string().repeat(width));
            }
            PatternField::Literal(literal) if index > 0 => {
                let after_a_kept = matches!(
                    fields[index - 1],
                    PatternField::Letter(letter, _) if wanted(letter)
                );
                let before_a_kept = fields[index + 1..].iter().any(
                    |field| matches!(field, PatternField::Letter(letter, _) if wanted(*letter)),
                );
                let quoted = |text: &str| format!("'{}'", text.replace('\'', "''"));
                if after_a_kept && before_a_kept {
                    result.push_str(&quoted(literal));
                } else if after_a_kept {
                    let ending: String = literal
                        .chars()
                        .take_while(|c| c.is_alphabetic() && !c.is_ascii())
                        .collect();
                    if !ending.is_empty() {
                        result.push_str(&quoted(&ending));
                    }
                }
            }
            _ => {}
        }
    }
    result
}

/// The input format of a locale's date pattern, as `datePatternAsInputFormat`: its fields and
/// delimiters, two-digit days and months and four-digit years.
pub fn date_pattern_as_input_format(locale_format: &str) -> DateInputFormat {
    let mut pattern = String::new();
    let mut last = None;
    for character in locale_format.chars() {
        match character {
            'd' | 'M' | 'y' if last == Some(character) => {}
            'd' | 'M' | 'y' => {
                // Fields with nothing between them (`My`, in some locales) are told apart.
                if last == Some('M') && character == 'y' {
                    pattern.push('/');
                }
                pattern.push_str(match character {
                    'd' => "dd",
                    'M' => "MM",
                    _ => "yyyy",
                });
                last = Some(character);
            }
            '/' | '-' | '.' => {
                pattern.push(character);
                last = None;
            }
            _ => {}
        }
    }
    let pattern = pattern.strip_suffix('.').map(str::to_string).unwrap_or(pattern);
    let delimiter = pattern.chars().find(|c| matches!(c, '/' | '-' | '.')).unwrap_or('/');
    DateInputFormat { pattern_with_delimiters: pattern, delimiter }
}

// ---- The calendar --------------------------------------------------------------------------

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn length_of_month(year: i32, month: i32) -> i32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 31,
    }
}

/// The days from 1970-01-01 to a date.
fn days_from_civil(year: i32, month: i32, day: i32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month_from_march = i64::from((month + 9) % 12);
    let day_of_year = (153 * month_from_march + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The date `days` after 1970-01-01.
fn civil_from_days(days: i64) -> (i32, i32, i32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 { month_from_march + 3 } else { month_from_march - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year as i32, month as i32, day as i32)
}

/// 1 being Monday; 1970-01-01 was a Thursday.
fn day_of_week_of_days(days: i64) -> i32 {
    (days + 3).rem_euclid(7) as i32 + 1
}

// ---- What's the locale's --------------------------------------------------------------------

/// A Unicode extension's value in a tag (`-u-nu-arab`).
fn unicode_extension(locale: &Locale, key: &str) -> Option<String> {
    let tag = locale.to_language_tag().to_ascii_lowercase();
    let mut subtags = tag.split('-').skip_while(|subtag| *subtag != "u").skip(1);
    subtags.find(|subtag| *subtag == key)?;
    subtags.next().map(str::to_string)
}

/// A locale's language, by its current name: `iw` is `he`.
fn language_of(locale: &Locale) -> String {
    let language = locale.language();
    if language == "iw" { "he".to_string() } else { language }
}

/// The digit 0 of a locale's numbers: its `-u-nu-`, or the language's and region's own.
fn zero_digit(locale: &Locale) -> char {
    match unicode_extension(locale, "nu").as_deref() {
        Some("arab") => '\u{0660}',
        Some("arabext") => '\u{06F0}',
        Some("deva") => '\u{0966}',
        Some("beng") => '\u{09E6}',
        Some(_) => '0',
        None if language_of(locale) == "ar"
            && !matches!(locale.region().as_str(), "AE" | "DZ" | "EH" | "LY" | "MA" | "TN") =>
        {
            '\u{0660}'
        }
        None => '0',
    }
}

/// The region a tag names, or the one its language is most spoken in, as ICU's likely subtags: from
/// memory of CLDR's table, where the JDK's `WeekFields` takes the US for a bare language.
fn region_or_likely(locale: &Locale) -> String {
    let region = locale.region();
    if !region.is_empty() {
        return region;
    }
    match language_of(locale).as_str() {
        "en" => "US",
        "es" => "ES",
        "da" => "DK",
        "zh" => "CN",
        "ko" => "KR",
        "ar" => "EG",
        "he" => "IL",
        "hi" => "IN",
        "ja" => "JP",
        "pt" => "BR",
        "th" => "TH",
        _ => "",
    }
    .to_string()
}

/// CLDR's first day of the week by region, 1 being Monday: the JDK 21's, all of its regions.
fn first_day_of_week(locale: &Locale) -> i32 {
    match region_or_likely(locale).as_str() {
        "AG" | "AS" | "BD" | "BR" | "BS" | "BT" | "BW" | "BZ" | "CA" | "CO" | "DM" | "DO"
        | "ET" | "GT" | "GU" | "HK" | "HN" | "ID" | "IL" | "IN" | "JM" | "JP" | "KE" | "KH"
        | "KR" | "LA" | "MH" | "MM" | "MO" | "MT" | "MX" | "MZ" | "NI" | "NP" | "PA" | "PE"
        | "PH" | "PK" | "PR" | "PT" | "PY" | "SA" | "SG" | "SV" | "TH" | "TT" | "TW" | "UM"
        | "US" | "VE" | "VI" | "WS" | "YE" | "ZA" | "ZW" => 7,
        "AE" | "AF" | "BH" | "DJ" | "DZ" | "EG" | "IQ" | "IR" | "JO" | "KW" | "LY" | "OM"
        | "QA" | "SD" | "SY" => 6,
        "MV" => 5,
        _ => 1,
    }
}

/// A locale's short date, as `getLocalizedDateTimePattern(SHORT, …)`; those told apart by the input
/// format they give, the JDK's for those named.
fn short_date_pattern(locale: &Locale) -> &'static str {
    let region = locale.region();
    match (language_of(locale).as_str(), region.as_str()) {
        ("en", "GB" | "AU" | "IE" | "IN" | "NZ" | "SG" | "HK") => "dd/MM/y",
        ("en", "CA") => "y-MM-dd",
        ("en", "ZA") => "y/MM/dd",
        ("es", "CL") => "dd-MM-yy",
        ("es", _) => "d/M/yy",
        ("da", _) => "dd.MM.y",
        ("zh", "HK") => "d/M/y",
        ("zh", _) => "y/M/d",
        ("ko", _) => "yy. M. d.",
        ("ar", _) => "d\u{200f}/M\u{200f}/y",
        ("he", _) => "d.M.y",
        _ => "M/d/yy",
    }
}

/// What a language calls dates, from CLDR.
struct LanguageData {
    months: [&'static str; 12],
    months_short: [&'static str; 12],
    /// Monday first, as what they're indexed by.
    weekdays: [&'static str; 7],
    weekdays_short: [&'static str; 7],
    weekdays_narrow: [&'static str; 7],
    year_month: &'static str,
    year_month_day: &'static str,
    year_month_weekday_day: &'static str,
    /// Whether a date standing alone starts with a capital, as ICU's
    /// `CAPITALIZATION_FOR_STANDALONE`.
    capitalized_alone: bool,
}

fn language_data(locale: &Locale) -> &'static LanguageData {
    match (language_of(locale).as_str(), locale.region().as_str()) {
        ("en", "GB" | "AU" | "IE" | "IN" | "NZ" | "HK" | "SG" | "ZA") => &ENGLISH_DAY_FIRST,
        ("es", _) => &SPANISH,
        ("da", _) => &DANISH,
        ("zh", _) => &CHINESE,
        ("ko", _) => &KOREAN,
        ("ar", _) => &ARABIC,
        _ => &ENGLISH,
    }
}

const ENGLISH_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const ENGLISH_MONTHS_SHORT: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const ENGLISH_WEEKDAYS: [&str; 7] =
    ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const ENGLISH_WEEKDAYS_SHORT: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const ENGLISH_WEEKDAYS_NARROW: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];

static ENGLISH: LanguageData = LanguageData {
    months: ENGLISH_MONTHS,
    months_short: ENGLISH_MONTHS_SHORT,
    weekdays: ENGLISH_WEEKDAYS,
    weekdays_short: ENGLISH_WEEKDAYS_SHORT,
    weekdays_narrow: ENGLISH_WEEKDAYS_NARROW,
    year_month: "MMMM y",
    year_month_day: "MMM d, y",
    year_month_weekday_day: "EEEE, MMMM d, y",
    capitalized_alone: false,
};

static ENGLISH_DAY_FIRST: LanguageData = LanguageData {
    months: ENGLISH_MONTHS,
    months_short: ENGLISH_MONTHS_SHORT,
    weekdays: ENGLISH_WEEKDAYS,
    weekdays_short: ENGLISH_WEEKDAYS_SHORT,
    weekdays_narrow: ENGLISH_WEEKDAYS_NARROW,
    year_month: "MMMM y",
    year_month_day: "d MMM y",
    year_month_weekday_day: "EEEE d MMMM y",
    capitalized_alone: false,
};

static SPANISH: LanguageData = LanguageData {
    months: [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ],
    months_short: [
        "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic",
    ],
    weekdays: ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"],
    weekdays_short: ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"],
    weekdays_narrow: ["L", "M", "X", "J", "V", "S", "D"],
    year_month: "MMMM 'de' y",
    year_month_day: "d MMM y",
    year_month_weekday_day: "EEEE, d 'de' MMMM 'de' y",
    capitalized_alone: true,
};

static DANISH: LanguageData = LanguageData {
    months: [
        "januar",
        "februar",
        "marts",
        "april",
        "maj",
        "juni",
        "juli",
        "august",
        "september",
        "oktober",
        "november",
        "december",
    ],
    months_short: [
        "jan.", "feb.", "mar.", "apr.", "maj", "jun.", "jul.", "aug.", "sep.", "okt.", "nov.",
        "dec.",
    ],
    weekdays: ["mandag", "tirsdag", "onsdag", "torsdag", "fredag", "lørdag", "søndag"],
    weekdays_short: ["man.", "tirs.", "ons.", "tors.", "fre.", "lør.", "søn."],
    weekdays_narrow: ["M", "T", "O", "T", "F", "L", "S"],
    year_month: "MMMM y",
    year_month_day: "d. MMM y",
    year_month_weekday_day: "EEEE 'den' d. MMMM y",
    capitalized_alone: false,
};

static CHINESE: LanguageData = LanguageData {
    months: [
        "一月",
        "二月",
        "三月",
        "四月",
        "五月",
        "六月",
        "七月",
        "八月",
        "九月",
        "十月",
        "十一月",
        "十二月",
    ],
    months_short: [
        "1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月",
    ],
    weekdays: ["星期一", "星期二", "星期三", "星期四", "星期五", "星期六", "星期日"],
    weekdays_short: ["周一", "周二", "周三", "周四", "周五", "周六", "周日"],
    weekdays_narrow: ["一", "二", "三", "四", "五", "六", "日"],
    year_month: "y年M月",
    year_month_day: "y年M月d日",
    year_month_weekday_day: "y年M月d日EEEE",
    capitalized_alone: false,
};

static KOREAN: LanguageData = LanguageData {
    months: ["1월", "2월", "3월", "4월", "5월", "6월", "7월", "8월", "9월", "10월", "11월", "12월"],
    months_short: [
        "1월", "2월", "3월", "4월", "5월", "6월", "7월", "8월", "9월", "10월", "11월", "12월",
    ],
    weekdays: ["월요일", "화요일", "수요일", "목요일", "금요일", "토요일", "일요일"],
    weekdays_short: ["월", "화", "수", "목", "금", "토", "일"],
    weekdays_narrow: ["월", "화", "수", "목", "금", "토", "일"],
    year_month: "y년 M월",
    year_month_day: "y년 M월 d일",
    year_month_weekday_day: "y년 M월 d일 EEEE",
    capitalized_alone: false,
};

static ARABIC: LanguageData = LanguageData {
    months: [
        "يناير",
        "فبراير",
        "مارس",
        "أبريل",
        "مايو",
        "يونيو",
        "يوليو",
        "أغسطس",
        "سبتمبر",
        "أكتوبر",
        "نوفمبر",
        "ديسمبر",
    ],
    months_short: [
        "يناير",
        "فبراير",
        "مارس",
        "أبريل",
        "مايو",
        "يونيو",
        "يوليو",
        "أغسطس",
        "سبتمبر",
        "أكتوبر",
        "نوفمبر",
        "ديسمبر",
    ],
    weekdays: ["الاثنين", "الثلاثاء", "الأربعاء", "الخميس", "الجمعة", "السبت", "الأحد"],
    weekdays_short: ["الاثنين", "الثلاثاء", "الأربعاء", "الخميس", "الجمعة", "السبت", "الأحد"],
    weekdays_narrow: ["ن", "ث", "ر", "خ", "ج", "س", "ح"],
    year_month: "MMMM y",
    year_month_day: "d MMM y",
    year_month_weekday_day: "EEEE، d MMMM y",
    capitalized_alone: false,
};
