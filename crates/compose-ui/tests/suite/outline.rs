//! Compose's `OutlineTest` (ui-graphics): an outline's bounds and equality, for a rectangle, a
//! rounded rectangle and a path.

use compose_ui::{Outline, Path, Rect};

fn triangle() -> Path {
    let path = Path::new();
    path.move_to(5.0, 15.0);
    path.line_to(100.0, 200.0);
    path.line_to(0.0, 200.0);
    path.close();
    path
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/OutlineTest.kt, testRectOutlineBounds
#[test]
fn test_rect_outline_bounds() {
    let outline = Outline::Rectangle(Rect::new(1.0, 2.0, 3.0, 4.0));
    assert_eq!(outline.bounds(), Rect::new(1.0, 2.0, 3.0, 4.0));
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/OutlineTest.kt, testRoundRectOutlineBounds
#[test]
fn test_round_rect_outline_bounds() {
    let outline = Outline::Rounded(Rect::new(5.0, 10.0, 15.0, 20.0), [7.0; 4]);
    assert_eq!(outline.bounds(), Rect::new(5.0, 10.0, 15.0, 20.0));
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/OutlineTest.kt, testPathOutlineBounds
#[test]
fn test_path_outline_bounds() {
    let outline = Outline::generic(&triangle());
    assert_eq!(outline.bounds(), Rect::new(0.0, 15.0, 100.0, 200.0));
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/OutlineTest.kt, testRectOutlineEquality
#[test]
fn test_rect_outline_equality() {
    let outline = Outline::Rectangle(Rect::new(1.0, 2.0, 3.0, 4.0));
    assert_eq!(outline, Outline::Rectangle(Rect::new(1.0, 2.0, 3.0, 4.0)));
    assert_ne!(outline, Outline::Rectangle(Rect::new(4.0, 3.0, 2.0, 1.0)));
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/OutlineTest.kt, testRoundRectOutlineEquality
#[test]
fn test_round_rect_outline_equality() {
    let outline = Outline::Rounded(Rect::new(5.0, 10.0, 15.0, 20.0), [7.0; 4]);
    assert_eq!(outline, Outline::Rounded(Rect::new(5.0, 10.0, 15.0, 20.0), [7.0; 4]));
    assert_ne!(outline, Outline::Rounded(Rect::new(20.0, 15.0, 10.0, 5.0), [3.0; 4]));
    // Each half alone makes them differ.
    assert_ne!(outline, Outline::Rounded(Rect::new(5.0, 10.0, 15.0, 20.0), [3.0; 4]));
    assert_ne!(outline, Outline::Rounded(Rect::new(20.0, 15.0, 10.0, 5.0), [7.0; 4]));
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/OutlineTest.kt, testPathOutlineEquality
#[test]
fn test_path_outline_equality() {
    // Compose's generic outlines are equal only to themselves, as their path can change after.
    // compose-rs's holds the path's contents when it was made (D1925), so two outlines of the
    // same contents are equal, and the path changing after doesn't change them.
    let path = triangle();
    let outline = Outline::generic(&path);
    assert_eq!(outline, outline.clone());
    assert_eq!(outline, Outline::generic(&path));
    assert_eq!(outline, Outline::generic(&triangle()), "distinct path with the same contents");
    path.line_to(50.0, 50.0);
    let changed = Outline::generic(&path);
    assert_ne!(outline, changed);
    assert_eq!(outline.bounds(), Rect::new(0.0, 15.0, 100.0, 200.0), "made before the change");
}
