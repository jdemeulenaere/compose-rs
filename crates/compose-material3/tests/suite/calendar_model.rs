//! The calendar model, as Compose's `CalendarModelTest` (the legacy model's half has no
//! counterpart here: there's one model).

use compose_material3::calendar_model::{
    CalendarDate, CalendarModel, format_with_skeleton, to_local_string,
};
use compose_ui::Locale;

const JANUARY_2022_MILLIS: i64 = 1640995200000;

fn model() -> CalendarModel {
    CalendarModel::new(Locale::new("en-US"))
}

fn january_first_2022() -> CalendarDate {
    CalendarDate { year: 2022, month: 1, day_of_month: 1, utc_time_millis: JANUARY_2022_MILLIS }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, dateCreation
#[test]
fn a_date_from_millis_is_its_year_month_and_day() {
    let date = model().get_canonical_date(JANUARY_2022_MILLIS);
    assert_eq!((date.year, date.month, date.day_of_month), (2022, 1, 1));
    assert_eq!(date.utc_time_millis, JANUARY_2022_MILLIS);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, dateCreation_withRounding
#[test]
fn a_date_from_millis_rounds_down_to_the_start_of_the_day() {
    let date = model().get_canonical_date(JANUARY_2022_MILLIS + 30000);
    assert_eq!((date.year, date.month, date.day_of_month), (2022, 1, 1));
    assert_eq!(date.utc_time_millis, JANUARY_2022_MILLIS);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, dateRestore
#[test]
fn a_date_comes_back_from_its_millis() {
    let date = january_first_2022();
    assert_eq!(model().get_canonical_date(date.utc_time_millis), date);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, monthCreation
#[test]
fn a_month_is_the_same_from_a_date_millis_or_year_and_month() {
    let model = model();
    let from_date = model.get_month_of_date(&january_first_2022());
    assert_eq!(from_date, model.get_month_of_millis(JANUARY_2022_MILLIS));
    assert_eq!(from_date, model.get_month(2022, 1));
    assert_eq!(from_date.number_of_days, 31);
    // January 2022 starts on a Saturday: six cells in a week starting on Sunday.
    assert_eq!(from_date.days_from_start_of_week_to_first_of_month, 6);
    assert_eq!(from_date.start_utc_time_millis, JANUARY_2022_MILLIS);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, monthCreation_withRounding
#[test]
fn a_month_from_millis_rounds_down() {
    let model = model();
    assert_eq!(
        model.get_month_of_date(&january_first_2022()),
        model.get_month_of_millis(JANUARY_2022_MILLIS + 10000)
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, monthRestore
#[test]
fn a_month_comes_back_from_its_start() {
    let model = model();
    let month = model.get_month(1999, 12);
    assert_eq!(model.get_month_of_millis(month.start_utc_time_millis), month);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, plusMinusMonth
#[test]
fn months_add_and_subtract() {
    let model = model();
    let month = model.get_month_of_millis(JANUARY_2022_MILLIS);
    let next = model.get_month_of_millis(month.end_utc_time_millis() + 1);
    let plus = model.plus_months(&month, 1);
    assert_eq!(plus, next);
    assert_eq!((plus.year, plus.month), (2022, 2));
    assert_eq!(model.minus_months(&plus, 1), month);
    // Across a year, and not at all for a count that isn't positive.
    assert_eq!(model.plus_months(&month, 12), model.get_month(2023, 1));
    assert_eq!(model.minus_months(&month, 1), model.get_month(2021, 12));
    assert_eq!(model.plus_months(&month, 0), month);
    assert_eq!(model.plus_months(&month, -1), month);
    assert_eq!(model.minus_months(&month, -1), month);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, parseDate
#[test]
fn a_date_parses_by_a_pattern() {
    let parsed = model().parse("1/1/2022", "M/d/yyyy", &Locale::new("en"));
    assert_eq!(parsed, Some(january_first_2022()));
    // What the input fields hand it: no delimiters.
    let parsed = model().parse("01012022", "MMddyyyy", &Locale::new("en"));
    assert_eq!(parsed, Some(january_first_2022()));
    // What doesn't read as the pattern doesn't parse.
    assert_eq!(model().parse("1/1/22", "M/d/yyyy", &Locale::new("en")), None);
    assert_eq!(model().parse("13/1/2022", "M/d/yyyy", &Locale::new("en")), None);
    assert_eq!(model().parse("1/32/2022", "M/d/yyyy", &Locale::new("en")), None);
    assert_eq!(model().parse("0/1/2022", "M/d/yyyy", &Locale::new("en")), None);
    assert_eq!(model().parse("1/1/2022x", "M/d/yyyy", &Locale::new("en")), None);
    // A day past its month's end is the month's last, as java.time's smart resolving.
    let parsed = model().parse("02/31/2022", "MM/dd/yyyy", &Locale::new("en")).unwrap();
    assert_eq!((parsed.month, parsed.day_of_month), (2, 28));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, parseDate_Arabic
#[test]
fn a_date_parses_in_the_locales_digits() {
    let parsed = model().parse("٠١/٠١/٢٠٢٢", "d/MM/yyyy", &Locale::new("ar-u-nu-arab"));
    assert_eq!(parsed, Some(january_first_2022()));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, formatDate
#[test]
fn a_date_formats_by_a_skeleton() {
    let model = model();
    let date = january_first_2022();
    assert_eq!(model.format_date_with_skeleton(&date, "yMMMd"), "Jan 1, 2022");
    assert_eq!(model.format_date_with_skeleton(&date, "dMMMy"), "Jan 1, 2022");
    assert_eq!(model.format_date_with_skeleton(&date, "yMMMMEEEEd"), "Saturday, January 1, 2022");
    assert_eq!(model.format_date_with_skeleton(&date, "yMMMd"), date.format(&model, "yMMMd"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, formatMonth
#[test]
fn a_month_formats_by_a_skeleton() {
    let model = model();
    let month = model.get_month(2022, 3);
    assert_eq!(model.format_month_with_skeleton(&month, "yMMMM"), "March 2022");
    assert_eq!(model.format_month_with_skeleton(&month, "MMMMy"), "March 2022");
    assert_eq!(model.format_month_with_skeleton(&month, "yMMMM"), month.format(&model, "yMMMM"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, formatWithSkeleton
#[test]
fn millis_format_by_a_skeleton() {
    let us = Locale::new("en-US");
    assert_eq!(
        format_with_skeleton(JANUARY_2022_MILLIS, "yMMMMEEEEd", &us),
        "Saturday, January 1, 2022"
    );
    assert_eq!(
        format_with_skeleton(JANUARY_2022_MILLIS - 1000000000, "yMMMMEEEEd", &us),
        "Monday, December 20, 2021"
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, formatWithSkeletonProducingEqualPattern
#[test]
fn a_skeleton_that_is_its_own_pattern_formats_as_it_is() {
    let us = Locale::new("en-US");
    assert_eq!(format_with_skeleton(JANUARY_2022_MILLIS, "YY", &us), "22");
    assert_eq!(format_with_skeleton(JANUARY_2022_MILLIS - 1000000000, "YY", &us), "21");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, formatWithSkeletonCapitalizationForLocale
#[test]
fn a_format_is_capitalized_as_the_locale_does_alone() {
    assert_eq!(
        format_with_skeleton(JANUARY_2022_MILLIS, "yMMMM", &Locale::new("en-US")),
        "January 2022"
    );
    assert_eq!(
        format_with_skeleton(JANUARY_2022_MILLIS, "yMMMM", &Locale::new("ES")),
        "Enero de 2022"
    );
    assert_eq!(
        format_with_skeleton(JANUARY_2022_MILLIS, "yMMMM", &Locale::new("DA")),
        "januar 2022"
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, weekdayNames
#[test]
fn weekday_names_start_on_monday() {
    let names = model().weekday_names();
    assert_eq!(names.len(), 7);
    assert!(names[0].0.contains("Monday"));
    for (full, narrow) in &names {
        assert_eq!(
            narrow.chars().next().unwrap().to_ascii_lowercase(),
            full.chars().next().unwrap().to_ascii_lowercase()
        );
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, dateInputFormat
#[test]
fn the_input_format_comes_from_the_locales_short_date() {
    for (tag, with_delimiters, without_delimiters, delimiter) in [
        ("en-US", "MM/dd/yyyy", "MMddyyyy", '/'),
        ("zh-CN", "yyyy/MM/dd", "yyyyMMdd", '/'),
        ("en-GB", "dd/MM/yyyy", "ddMMyyyy", '/'),
        ("ko-KR", "yyyy.MM.dd", "yyyyMMdd", '.'),
        ("es-CL", "dd-MM-yyyy", "ddMMyyyy", '-'),
    ] {
        let format = CalendarModel::new(Locale::new(tag)).get_date_input_format();
        assert_eq!(format.pattern_with_delimiters, with_delimiters, "{tag}");
        assert_eq!(format.pattern_without_delimiters(), without_delimiters, "{tag}");
        assert_eq!(format.delimiter, delimiter, "{tag}");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CalendarModelTest.kt, toLocalStringFormat
#[test]
fn numbers_read_in_the_locales_digits() {
    assert_eq!(to_local_string(2, 1, 40, &Locale::new("en")), "2");
    assert_eq!(to_local_string(2, 1, 40, &Locale::new("ar-u-nu-arab")), "٢");
    assert_eq!(to_local_string(2, 1, 40, &Locale::new("ar-u-nu-latn")), "2");
    // The digits asked for, as `minDigits` and `maxDigits`.
    assert_eq!(to_local_string(7, 2, 2, &Locale::new("en")), "07");
    assert_eq!(to_local_string(2022, 1, 2, &Locale::new("en")), "22");
}

#[test]
fn the_first_day_of_the_week_is_the_regions() {
    for (tag, first_day, cells) in
        [("en-US", 7, 6), ("en-GB", 1, 5), ("zh-CN", 7, 6), ("ar-EG", 6, 0)]
    {
        let model = CalendarModel::new(Locale::new(tag));
        assert_eq!(model.first_day_of_week(), first_day, "{tag}");
        assert_eq!(
            model.get_month(2022, 1).days_from_start_of_week_to_first_of_month,
            cells,
            "{tag}"
        );
    }
    assert_eq!(model().get_day_of_week(&january_first_2022()), 6);
    // An extension's subtags aren't a region: Arabic's likely one is.
    assert_eq!(Locale::new("ar-u-nu-arab").region(), "");
    assert_eq!(CalendarModel::new(Locale::new("ar-u-nu-arab")).first_day_of_week(), 6);
}

#[test]
fn days_around_the_epoch_and_leap_years_are_right() {
    let model = model();
    let before = model.get_canonical_date(-1);
    assert_eq!((before.year, before.month, before.day_of_month), (1969, 12, 31));
    assert_eq!(before.utc_time_millis, -86_400_000);
    assert_eq!(model.get_month(2024, 2).number_of_days, 29);
    assert_eq!(model.get_month(1900, 2).number_of_days, 28);
    assert_eq!(model.get_month(2000, 2).number_of_days, 29);
    assert_eq!(model.get_month(1900, 1).index_in(&(1900..=2100)), 0);
    assert_eq!(model.get_month(1901, 3).index_in(&(1900..=2100)), 14);
    let month = model.get_month(2022, 2);
    assert_eq!(month.end_utc_time_millis(), month.start_utc_time_millis + 28 * 86_400_000 - 1);
}

#[test]
fn a_pattern_formats_in_the_locale() {
    let model = model();
    let spanish = Locale::new("es");
    assert_eq!(model.format_with_pattern(JANUARY_2022_MILLIS, "MMddyyyy", &spanish), "01012022");
    assert_eq!(
        model.format_with_pattern(JANUARY_2022_MILLIS, "EEE d 'de' MMMM", &spanish),
        "sáb 1 de enero"
    );
    assert_eq!(
        model.format_with_pattern(JANUARY_2022_MILLIS, "d/M/yy", &Locale::new("ar-u-nu-arab")),
        "١/١/٢٢"
    );
}
