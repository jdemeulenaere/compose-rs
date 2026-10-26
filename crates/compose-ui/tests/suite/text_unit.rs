//! `TextUnit`, adapted from Compose's `TextUnitTest` (ui-unit).

use compose_ui::{TextUnit, TextUnitType, UnitExt};

#[track_caller]
fn assert_unit(unit: TextUnit, unit_type: TextUnitType, value: f32) {
    assert_eq!(unit.unit_type(), unit_type);
    assert_eq!(unit.is_sp(), unit_type == TextUnitType::Sp);
    assert_eq!(unit.is_em(), unit_type == TextUnitType::Em);
    assert_eq!(unit.is_unspecified(), unit_type == TextUnitType::Unspecified);
    assert_eq!(unit.value(), value);
}

fn panics(operation: impl FnOnce() + std::panic::UnwindSafe) -> bool {
    std::panic::catch_unwind(operation).is_err()
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, construct_sp_from_float
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, construct_sp_from_int
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, construct_sp_from_double
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, construct_sp_from_float_extension
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, construct_sp_from_int_extension
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, construct_sp_from_double_extension
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, construct_em_from_float
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, construct_em_from_int
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, construct_em_from_double
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, construct_em_from_float_extension
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, construct_em_from_int_extension
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, construct_em_from_double_extension
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, unspecified_type_check
#[test]
fn units_are_made_from_any_number() {
    for unit in
        [5f32.sp(), 5.sp(), 5.0f64.sp(), TextUnit::sp(5.0), TextUnit::new(5.0, TextUnitType::Sp)]
    {
        assert_unit(unit, TextUnitType::Sp, 5.0);
    }
    for unit in
        [5f32.em(), 5.em(), 5.0f64.em(), TextUnit::em(5.0), TextUnit::new(5.0, TextUnitType::Em)]
    {
        assert_unit(unit, TextUnitType::Em, 5.0);
    }
    assert_eq!(TextUnit::UNSPECIFIED.unit_type(), TextUnitType::Unspecified);
    assert!(TextUnit::UNSPECIFIED.is_unspecified());
    assert!(!TextUnit::UNSPECIFIED.is_sp() && !TextUnit::UNSPECIFIED.is_em());
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, minus_em
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, minus_sp
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, minus_unspecified
#[test]
fn negation_keeps_the_type() {
    assert_unit(-(1.em()), TextUnitType::Em, -1.0);
    assert_unit(-(1.sp()), TextUnitType::Sp, -1.0);
    assert!(panics(|| {
        let _ = -TextUnit::UNSPECIFIED;
    }));
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_sp_float
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_sp_double
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_sp_int
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_float_sp
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_double_sp
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_int_sp
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_float_em
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_double_em
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_int_em
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_em_float
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_em_double
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_em_int
#[test]
fn multiplication_keeps_the_type() {
    for unit in
        [2.sp() * 3f32, 2.sp() * 3.0f64, 2.sp() * 3, 2f32 * 3.sp(), 2.0f64 * 3.sp(), 2 * 3.sp()]
    {
        assert_unit(unit, TextUnitType::Sp, 6.0);
    }
    for unit in
        [2f32 * 3.em(), 2.0f64 * 3.em(), 2 * 3.em(), 2.em() * 3f32, 2.em() * 3.0f64, 2.em() * 3]
    {
        assert_unit(unit, TextUnitType::Em, 6.0);
    }
    assert_unit(6.sp() / 3f32, TextUnitType::Sp, 2.0);
    assert_unit(6.em() / 3, TextUnitType::Em, 2.0);
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_unspecified_float
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_unspecified_double
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_unspecified_int
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_float_unspecified
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_double_unspecified
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, multiply_int_unspecified
#[test]
fn multiplying_unspecified_panics() {
    let unspecified = TextUnit::UNSPECIFIED;
    assert!(panics(|| {
        let _ = unspecified * 3f32;
    }));
    assert!(panics(|| {
        let _ = unspecified * 3.0f64;
    }));
    assert!(panics(|| {
        let _ = unspecified * 3;
    }));
    assert!(panics(|| {
        let _ = 3f32 * unspecified;
    }));
    assert!(panics(|| {
        let _ = 3.0f64 * unspecified;
    }));
    assert!(panics(|| {
        let _ = 3 * unspecified;
    }));
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, compare_sp_sp
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, compare_em_em
#[test]
#[allow(clippy::neg_cmp_op_on_partial_ord, reason = "the operators are what's tested")]
fn units_of_a_type_compare_by_value() {
    for unit in [UnitExt::sp, UnitExt::em] {
        let (one, two) = (unit(1), unit(2));
        assert!(!(one > two) && !(one >= two) && one < two && one <= two && one != two);
        assert!(two > one && two >= one && !(two < one) && !(two <= one) && two != one);
        assert!(!(two > two) && two >= two && !(two < two) && two <= two && two == two);
    }
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, compare_sp_em
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, compare_em_sp
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, compare_unspecified_sp
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, compare_sp_unspecified
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, compare_unspecified_em
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, compare_em_unspecified
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, compare_unspecified_unspecified
#[test]
fn comparing_other_types_or_unspecified_panics() {
    let unspecified = TextUnit::UNSPECIFIED;
    let pairs = [
        (1.sp(), 2.em()),
        (1.em(), 2.sp()),
        (unspecified, 2.sp()),
        (1.sp(), unspecified),
        (unspecified, 2.em()),
        (1.em(), unspecified),
        (unspecified, unspecified),
    ];
    for (a, b) in pairs {
        assert!(
            panics(move || {
                let _ = a > b;
            }),
            "{a:?} > {b:?}"
        );
    }
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, equals_sp_sp
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, equals_em_em
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, equals_unspecified_unspecified
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, equals_sp_em
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, equals_em_sp
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, equals_sp_unspecified
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, equals_unspecified_sp
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, equals_em_unspecified
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, equals_unspecified_em
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, unspecified_value_equals_nan
#[test]
fn units_are_equal_with_the_same_type_and_value() {
    assert_eq!(2.sp(), 2.0f64.sp());
    assert_ne!(2.sp(), 1.0f64.sp());
    assert_eq!(2.em(), 2.0f64.em());
    assert_ne!(2.em(), 1.0f64.em());
    assert_eq!(TextUnit::UNSPECIFIED, TextUnit::UNSPECIFIED);
    assert_ne!(2.sp(), 2.0f64.em());
    assert_ne!(2.sp(), 1.0f64.em());
    assert_ne!(2.em(), 2.0f64.sp());
    assert_ne!(2.em(), 1.0f64.sp());
    assert_ne!(2.sp(), TextUnit::UNSPECIFIED);
    assert_ne!(TextUnit::UNSPECIFIED, 2.sp());
    assert_ne!(2.em(), TextUnit::UNSPECIFIED);
    assert_ne!(TextUnit::UNSPECIFIED, 2.em());
    assert!(TextUnit::UNSPECIFIED.value().is_nan());
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, testIsSpecified
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, testIsUnspecified
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, testTakeOrElseTrue
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/TextUnitTest.kt, testTakeOrElseFalse
#[test]
fn an_unspecified_unit_takes_the_other() {
    assert!(!TextUnit::UNSPECIFIED.is_specified() && 1.sp().is_specified());
    assert!(TextUnit::UNSPECIFIED.is_unspecified() && !1.sp().is_unspecified());
    assert!(1.sp().or(TextUnit::UNSPECIFIED).is_specified());
    assert!(TextUnit::UNSPECIFIED.or(1.sp()).is_specified());
}

#[test]
fn units_lerp_within_a_type() {
    assert_eq!(TextUnit::lerp(10.sp(), 20.sp(), 0.5), 15.sp());
    assert!(panics(|| {
        let _ = TextUnit::lerp(10.sp(), 20.em(), 0.5);
    }));
}
