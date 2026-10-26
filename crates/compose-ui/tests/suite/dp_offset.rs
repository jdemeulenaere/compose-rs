//! Compose's `DpOffsetTest.kt` (ui-unit), adapted: `DpOffset`'s construction, copies,
//! arithmetic, interpolation and unspecified value.

use compose_ui::{Dp, DpOffset, UnitExt};

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, constructor
#[test]
fn an_offset_keeps_its_coordinates() {
    let offset = DpOffset::new(5.dp(), 10.dp());
    assert_eq!((offset.x, offset.y), (5.dp(), 10.dp()));
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, copy
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, copyOverwritesX
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, copyOverwritesY
#[test]
fn a_copy_changes_only_what_it_names() {
    let offset = DpOffset::new(12.dp(), 27.dp());
    assert_eq!(DpOffset { ..offset }, offset);
    assert_eq!(DpOffset { x: 59.dp(), ..offset }, DpOffset::new(59.dp(), 27.dp()));
    assert_eq!(DpOffset { y: 67.dp(), ..offset }, DpOffset::new(12.dp(), 67.dp()));
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, dpOffsetPlusDpOffset
#[test]
fn offsets_add_up() {
    let (a, b) = (DpOffset::new(3.dp(), 10.dp()), DpOffset::new(5.dp(), 8.dp()));
    assert_eq!(a + b, DpOffset::new(8.dp(), 18.dp()));
    assert_eq!(b + a, DpOffset::new(8.dp(), 18.dp()));
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, dpOffsetMinusDpOffset
#[test]
fn offsets_subtract() {
    let (a, b) = (DpOffset::new(3.dp(), 10.dp()), DpOffset::new(5.dp(), 8.dp()));
    assert_eq!(a - b, DpOffset::new(Dp(-2.0), 2.dp()));
    assert_eq!(b - a, DpOffset::new(2.dp(), Dp(-2.0)));
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, lerp
#[test]
fn offsets_interpolate() {
    let (a, b) = (DpOffset::new(3.dp(), 10.dp()), DpOffset::new(5.dp(), 8.dp()));
    assert_eq!(DpOffset::lerp(a, b, 0.5), DpOffset::new(4.dp(), 9.dp()));
    assert_eq!(DpOffset::lerp(a, b, 0.0), a);
    assert_eq!(DpOffset::lerp(a, b, 1.0), b);
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, testIsSpecified
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, testIsUnspecified
#[test]
fn only_the_unspecified_offset_is_unspecified() {
    assert!(!DpOffset::UNSPECIFIED.is_specified());
    assert!(DpOffset::UNSPECIFIED.is_unspecified());
    assert!(DpOffset::new(1.dp(), 1.dp()).is_specified());
    assert!(!DpOffset::new(1.dp(), 1.dp()).is_unspecified());
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, testTakeOrElseTrue
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, testTakeOrElseFalse
#[test]
fn take_or_else_replaces_only_an_unspecified_offset() {
    let specified = DpOffset::new(1.dp(), 1.dp());
    assert_eq!(specified.take_or_else(|| DpOffset::UNSPECIFIED), specified);
    assert_eq!(DpOffset::UNSPECIFIED.take_or_else(|| specified), specified);
}

// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, testToString
// Compose: compose/ui/ui-unit/src/commonTest/kotlin/androidx/compose/ui/unit/DpOffsetTest.kt, testUnspecifiedToString
#[test]
fn an_offset_prints_as_composes() {
    assert_eq!(format!("{:?}", DpOffset::new(1.dp(), 1.dp())), "(1.0.dp, 1.0.dp)");
    assert_eq!(format!("{:?}", DpOffset::UNSPECIFIED), "DpOffset.Unspecified");
}
