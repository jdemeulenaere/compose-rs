//! `paddingFrom`, `paddingFromBaseline`, `alignBy` and a text's baselines, as foundation-layout's
//! `AlignmentLineTest`, `RowColumnTest` and `RowColumnModifierTest`. Density 1.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    AlignmentLine, Constraints, Dp, FIRST_BASELINE, HorizontalAlignmentLine, IntSize,
    LAST_BASELINE, Layout, LayoutCoordinates, LayoutDirection, Offset, TextUnit, UnitExt,
    VerticalAlignmentLine, empty_measure_policy, measure_policy,
};

fn dp(pixels: i32) -> Dp {
    (pixels as f32).dp()
}

/// As Compose's `AlignmentLineLayout`: a leaf of the size with `line` where it says.
fn line_layout(
    width: i32,
    height: i32,
    line: impl Into<AlignmentLine>,
    at: i32,
    modifier: Modifier,
) {
    let line = line.into();
    let policy = measure_policy(move |scope, _, constraints| {
        scope.provide_alignment_lines([(line, at)]);
        let width = width.clamp(constraints.min_width, constraints.max_width);
        let height = height.clamp(constraints.min_height, constraints.max_height);
        scope.layout(width, height)
    });
    Layout(modifier, policy, callback(|| {}));
}

/// The parent's size and where the child goes, and its size, for a child with `content`'s modifier.
#[derive(Clone, Default)]
struct Seen {
    parent: Rc<Cell<IntSize>>,
    child: Rc<Cell<IntSize>>,
    at: Rc<Cell<Offset>>,
}

impl Seen {
    fn child_modifier(&self) -> Modifier {
        let (child, at) = (self.child.clone(), self.at.clone());
        Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
            child.set(coordinates.size());
            at.set(coordinates.position_in_root());
        })
    }

    fn parent_modifier(&self) -> Modifier {
        let parent = self.parent.clone();
        Modifier.on_size_changed(move |size: IntSize| parent.set(size))
    }
}

/// A child of `(child, 0)` or `(0, child)` with its line at `line_at`, with `padding` inside a box.
fn padded(
    vertical: bool,
    child: i32,
    line_at: i32,
    padding: impl Fn(AlignmentLine) -> Modifier + 'static,
) -> Seen {
    let seen = Seen::default();
    let s = seen.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let s = s.clone();
        let line: AlignmentLine = if vertical {
            VerticalAlignmentLine::new(i32::min).into()
        } else {
            HorizontalAlignmentLine::new(i32::min).into()
        };
        let modifier = padding(line).then(s.child_modifier());
        Box!(modifier = s.parent_modifier(), {
            let (width, height) = if vertical { (child, 0) } else { (0, child) };
            line_layout(width, height, line, line_at, modifier.clone());
        });
    });
    ui.wait_for_idle();
    seen
}

fn from_line(before: i32, after: i32) -> impl Fn(AlignmentLine) -> Modifier + 'static {
    move |line| Modifier.padding_from(line, dp(before), dp(after))
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFrom_vertical
#[test]
fn padding_from_a_vertical_line() {
    let seen = padded(true, 30, 10, from_line(20, 40));
    assert_eq!(seen.parent.get().width, 60);
    assert_eq!(seen.child.get().height, seen.parent.get().height);
    assert_eq!(seen.at.get(), Offset::new(10.0, 0.0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFrom_horizontal
#[test]
fn padding_from_a_horizontal_line() {
    let seen = padded(false, 30, 10, from_line(20, 40));
    assert_eq!(seen.parent.get().height, 60);
    assert_eq!(seen.child.get().width, seen.parent.get().width);
    assert_eq!(seen.at.get(), Offset::new(0.0, 10.0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFrom_vertical_withSmallOffsets
#[test]
fn padding_smaller_than_what_s_there_adds_nothing_vertical() {
    let seen = padded(true, 30, 10, from_line(5, 5));
    assert_eq!(seen.child.get(), seen.parent.get());
    assert_eq!(seen.at.get(), Offset::ZERO);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFrom_horizontal_withSmallOffsets
#[test]
fn padding_smaller_than_what_s_there_adds_nothing_horizontal() {
    let seen = padded(false, 30, 10, from_line(5, 5));
    assert_eq!(seen.child.get(), seen.parent.get());
    assert_eq!(seen.at.get(), Offset::ZERO);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFrom_vertical_withInsufficientSpace
#[test]
fn too_little_room_takes_from_the_padding_vertical() {
    let seen = padded(true, 25, 10, |line| {
        Modifier.width_in(Dp(0.0), dp(30)).padding_from(line, dp(20), dp(20))
    });
    assert_eq!(seen.parent.get().width, 30);
    assert_eq!(seen.at.get(), Offset::new(5.0, 0.0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFrom_horizontal_withInsufficientSpace
#[test]
fn too_little_room_takes_from_the_padding_horizontal() {
    let seen = padded(false, 25, 10, |line| {
        Modifier.height_in(Dp(0.0), dp(30)).padding_from(line, dp(20), dp(20))
    });
    assert_eq!(seen.parent.get().height, 30);
    assert_eq!(seen.at.get(), Offset::new(0.0, 5.0));
}

/// What a child's told the constraints are, through `padding_from` a line of `vertical`.
fn constraints_through(vertical: bool, modifier: Modifier) -> Constraints {
    let seen = Rc::new(Cell::new(Constraints::fixed(0, 0)));
    let s = seen.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let (s, modifier) = (s.clone(), modifier.clone());
        let line: AlignmentLine = if vertical {
            VerticalAlignmentLine::new(i32::min).into()
        } else {
            HorizontalAlignmentLine::new(i32::min).into()
        };
        Box!({
            let s = s.clone();
            let policy = measure_policy(move |scope, _, constraints| {
                s.set(constraints);
                scope.layout(0, 0)
            });
            Layout(
                modifier.clone().padding_from(line, dp(0), Dp::UNSPECIFIED),
                policy,
                callback(|| {}),
            );
        });
    });
    ui.wait_for_idle();
    seen.get()
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFrom_vertical_keepsCrossAxisMinConstraints
#[test]
fn padding_from_a_vertical_line_keeps_the_minimum_height() {
    let constraints = constraints_through(true, Modifier.height_in(dp(10), Dp::UNSPECIFIED));
    assert_eq!(constraints.min_height, 10);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFrom_horizontal_keepsCrossAxisMinConstraints
#[test]
fn padding_from_a_horizontal_line_keeps_the_minimum_width() {
    let constraints = constraints_through(false, Modifier.width_in(dp(10), Dp::UNSPECIFIED));
    assert_eq!(constraints.min_width, 10);
}

fn parent_position(coordinates: &LayoutCoordinates) -> Offset {
    let parent = coordinates.parent_layout_coordinates().expect("in a box");
    parent.local_position_of(coordinates, Offset::ZERO)
}

/// Two of a child of 20 and its line at 10, in 50: one before 20 from the line, one after.
fn min_constrained(vertical: bool) -> (Offset, Offset) {
    let (first, second) = (Rc::new(Cell::new(Offset::ZERO)), Rc::new(Cell::new(Offset::ZERO)));
    let (f, s) = (first.clone(), second.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let (f, s) = (f.clone(), s.clone());
        let line: AlignmentLine = if vertical {
            VerticalAlignmentLine::new(i32::min).into()
        } else {
            HorizontalAlignmentLine::new(i32::min).into()
        };
        let sized = move || if vertical { Modifier.width(dp(50)) } else { Modifier.height(dp(50)) };
        Box!({
            let (f, s) = (f.clone(), s.clone());
            line_layout(
                20,
                20,
                line,
                10,
                sized().padding_from(line, dp(20), Dp::UNSPECIFIED).on_globally_positioned(
                    move |coordinates: LayoutCoordinates| f.set(parent_position(&coordinates)),
                ),
            );
            line_layout(
                20,
                20,
                line,
                10,
                sized().padding_from(line, Dp::UNSPECIFIED, dp(20)).on_globally_positioned(
                    move |coordinates: LayoutCoordinates| s.set(parent_position(&coordinates)),
                ),
            );
        });
    });
    ui.wait_for_idle();
    (first.get(), second.get())
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFrom_vertical_whenMinConstrained
#[test]
fn padding_from_a_vertical_line_in_more_room_than_it_takes() {
    let (first, second) = min_constrained(true);
    assert_eq!((first.x, second.x), (10.0, 20.0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFrom_horizontal_whenMinConstrained
#[test]
fn padding_from_a_horizontal_line_in_more_room_than_it_takes() {
    let (first, second) = min_constrained(false);
    assert_eq!((first.y, second.y), (10.0, 20.0));
}

/// A box of two layouts of 30, the first baseline at 10 and the last at 20, the padding on the box.
fn baselines_in(modifier: Modifier) -> IntSize {
    let size = Rc::new(Cell::new(IntSize::ZERO));
    let s = size.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let (s, modifier) = (s.clone(), modifier.clone());
        Box!(modifier = Modifier.on_size_changed(move |size: IntSize| s.set(size)), {
            Box!(modifier = modifier.clone(), {
                line_layout(30, 30, FIRST_BASELINE, 10, Modifier);
                line_layout(30, 30, LAST_BASELINE, 20, Modifier);
            });
        });
    });
    ui.wait_for_idle();
    size.get()
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFromBaseline
#[test]
fn padding_from_the_baselines() {
    let size = baselines_in(Modifier.padding_from_baseline(dp(20), dp(20)));
    assert_eq!(size.height, 30 + (20 - 10) * 2);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFromBaseline_textUnit
#[test]
fn padding_from_the_baselines_in_sp() {
    let size =
        baselines_in(Modifier.padding_from_baseline(TextUnit::sp(20.0), TextUnit::UNSPECIFIED));
    assert_eq!(size.height, 30 + 20 - 10);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt, testPaddingFromBaseline_whenMinConstrained
#[test]
fn padding_from_the_baselines_in_more_room_than_they_take() {
    let at = Rc::new(Cell::new(Offset::ZERO));
    let a = at.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let a = a.clone();
        Box!({
            let a = a.clone();
            let modifier = Modifier
                .height(dp(50))
                .padding_from_baseline(dp(20), dp(20))
                .on_globally_positioned(move |coordinates: LayoutCoordinates| {
                    a.set(parent_position(&coordinates));
                });
            Box!(modifier = modifier, {
                line_layout(20, 20, FIRST_BASELINE, 10, Modifier);
                line_layout(20, 20, LAST_BASELINE, 10, Modifier);
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(at.get().y, 10.0);
}

#[test]
#[should_panic(expected = "non-negative")]
fn padding_from_a_line_can_t_be_negative() {
    let _ = Modifier.padding_from(FIRST_BASELINE, dp(-1), Dp::UNSPECIFIED);
}

/// What `children` place in a row or column, each noted by index.
#[derive(Clone)]
struct Places(Rc<std::cell::RefCell<Vec<Offset>>>);

impl Places {
    fn new(count: usize) -> Places {
        Places(Rc::new(std::cell::RefCell::new(vec![Offset::ZERO; count])))
    }

    fn at(&self, index: usize) -> Modifier {
        let places = self.0.clone();
        Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
            places.borrow_mut()[index] = coordinates.position_in_root();
        })
    }

    fn all(&self) -> Vec<Offset> {
        self.0.borrow().clone()
    }
}

fn point(x: i32, y: i32) -> Offset {
    Offset::new(x as f32, y as f32)
}

fn leaf(modifier: Modifier) {
    Layout(modifier, empty_measure_policy(), callback(|| {}));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withAlignByModifier
#[test]
fn a_row_aligns_by_lines_and_blocks() {
    let places = Places::new(5);
    let p = places.clone();
    let line = HorizontalAlignmentLine::new(i32::min);
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let p = p.clone();
        Row!(modifier = Modifier.fill_max_height(), |scope| {
            let size = Modifier.size(dp(40));
            line_layout(40, 40, line, 30, Modifier.align_by(scope, line).then(p.at(0)));
            leaf(
                Modifier
                    .align_by_block(scope, |measured| measured.measured_height() / 2)
                    .then(p.at(1))
                    .then(size.clone()),
            );
            line_layout(40, 40, line, 25, Modifier.align_by(scope, line).then(p.at(2)));
            leaf(
                Modifier
                    .align_by_block(scope, |measured| measured.measured_height() * 3 / 4)
                    .then(p.at(3))
                    .then(size.clone()),
            );
            line_layout(
                40,
                40,
                FIRST_BASELINE,
                20,
                Modifier.align_by_baseline(scope).then(p.at(4)),
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(
        places.all(),
        [point(0, 0), point(40, 10), point(80, 5), point(120, 0), point(160, 10)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withAlignByModifier_andWeight
#[test]
fn a_row_aligns_a_weighted_child_by_its_line() {
    let places = Places::new(2);
    let p = places.clone();
    let line = HorizontalAlignmentLine::new(i32::min);
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let p = p.clone();
        Row!(modifier = Modifier.fill_max_height(), |scope| {
            line_layout(40, 40, line, 30, Modifier.align_by(scope, line).then(p.at(0)));
            leaf(
                Modifier
                    .align_by_block(scope, |measured| measured.measured_height() / 2)
                    .weight(scope, 1.0)
                    .then(p.at(1))
                    .then(Modifier.height(dp(40))),
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(places.all(), [point(0, 0), point(40, 10)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withAlignByModifier
#[test]
fn a_column_aligns_by_lines_and_blocks() {
    let places = Places::new(4);
    let p = places.clone();
    let line = VerticalAlignmentLine::new(i32::min);
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let p = p.clone();
        Column!(modifier = Modifier.fill_max_width(), |scope| {
            let size = Modifier.size(dp(40));
            leaf(
                Modifier
                    .align_by_block(scope, |measured| measured.measured_width())
                    .then(p.at(0))
                    .then(size.clone()),
            );
            leaf(Modifier.align_by_block(scope, |_| 0).then(p.at(1)).then(size.clone()));
            line_layout(40, 40, line, 30, Modifier.align_by(scope, line).then(p.at(2)));
            line_layout(40, 40, line, 20, Modifier.align_by(scope, line).then(p.at(3)));
        });
    });
    ui.wait_for_idle();
    assert_eq!(places.all(), [point(0, 0), point(40, 40), point(10, 80), point(20, 120)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_Rtl_gravityAlignBy
#[test]
fn a_column_aligns_by_lines_from_the_right_right_to_left() {
    let places = Places::new(2);
    let p = places.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let p = p.clone();
        CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl),
            move || {
                let p = p.clone();
                Column!(modifier = Modifier.width(dp(100)), |scope| {
                    leaf(
                        Modifier
                            .align_by_block(scope, |measured| measured.measured_width())
                            .then(p.at(0))
                            .then(Modifier.size(dp(40))),
                    );
                    leaf(
                        Modifier
                            .align_by_block(scope, |measured| measured.measured_height() / 2)
                            .then(p.at(1))
                            .then(Modifier.size(dp(40))),
                    );
                });
            },
        );
    });
    ui.wait_for_idle();
    // Its first child's 40 before the line and the second's 20: from the right edge, 60 across.
    assert_eq!(places.all(), [point(60, 0), point(40, 40)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testAlignByModifiersChain_leftMostWins
#[test]
fn the_outermost_align_by_wins() {
    let places = Places::new(1);
    let p = places.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let p = p.clone();
        Row!(|scope| {
            leaf(
                Modifier
                    .align_by_block(scope, |measured| measured.measured_height())
                    .then(Modifier.size(dp(40))),
            );
            leaf(
                Modifier
                    .align_by_block(scope, |_| 0)
                    .align_by_block(scope, |measured| measured.measured_height() / 2)
                    .then(p.at(0))
                    .then(Modifier.size(dp(40))),
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(places.all(), [point(40, 40)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnModifierTest.kt, testRow_updatesOnAlignByBlockChange
#[test]
fn a_row_hears_of_a_new_block() {
    let which = mutable_state_of(false);
    let places = Places::new(1);
    let (w, p) = (which.handle(), places.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let p = p.clone();
        Box!(modifier = Modifier.size(dp(100)), {
            let p = p.clone();
            Row!(modifier = Modifier.wrap_content_height(), |scope| {
                for index in 0..5 {
                    let tall = if index == 4 { 10 } else { 20 };
                    let block: Modifier = if index == 4 && w.get() {
                        Modifier.align_by_block(scope, |_| 0)
                    } else {
                        Modifier.align_by_block(scope, |_| 5)
                    };
                    let noted = if index == 4 { p.at(0) } else { Modifier };
                    leaf(Modifier.size((dp(20), dp(tall))).then(block).then(noted));
                }
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(places.all()[0].y, 0.0);
    which.set(true);
    ui.wait_for_idle();
    assert_eq!(places.all()[0].y, 5.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnModifierTest.kt, testColumn_updatesOnAlignByBlockChange
#[test]
fn a_column_hears_of_a_new_block() {
    let which = mutable_state_of(false);
    let places = Places::new(1);
    let (w, p) = (which.handle(), places.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let p = p.clone();
        Box!(modifier = Modifier.size(dp(100)), {
            let p = p.clone();
            Column!(modifier = Modifier.wrap_content_width(), |scope| {
                for index in 0..5 {
                    let wide = if index == 4 { 10 } else { 20 };
                    let block: Modifier = if index == 4 && w.get() {
                        Modifier.align_by_block(scope, |_| 0)
                    } else {
                        Modifier.align_by_block(scope, |_| 5)
                    };
                    let noted = if index == 4 { p.at(0) } else { Modifier };
                    leaf(Modifier.size((dp(wide), dp(20))).then(block).then(noted));
                }
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(places.all()[0].x, 0.0);
    which.set(true);
    ui.wait_for_idle();
    assert_eq!(places.all()[0].x, 5.0);
}

/// Texts of different sizes side by side sit on one line.
#[test]
fn a_text_gives_its_baselines() {
    let places = Places::new(2);
    let p = places.clone();
    let seen = Rc::new(Cell::new((0, 0)));
    let s = seen.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let (p, s) = (p.clone(), s.clone());
        Row!(|scope| {
            let s = s.clone();
            BasicText!(
                "small",
                modifier = Modifier
                    .align_by_block(scope, move |measured| {
                        let first = measured.alignment_line(FIRST_BASELINE);
                        s.set((first, measured.alignment_line(LAST_BASELINE)));
                        first
                    })
                    .then(p.at(0))
            );
            BasicText!("other", modifier = Modifier.align_by_baseline(scope).then(p.at(1)));
        });
    });
    ui.wait_for_idle();
    let (first, last) = seen.get();
    assert!(first > 0 && last >= first, "{first} {last}");
    // Same style, same line: the same place.
    assert_eq!(places.all()[0].y, places.all()[1].y);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, coordinatesWhileAligning
#[test]
fn coordinates_are_none_while_aligning_then_there() {
    let located = Rc::new(std::cell::RefCell::new(Vec::new()));
    let l = located.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let l = l.clone();
        Row!(modifier = Modifier.fill_max_size(), |scope| {
            Box!(modifier = Modifier.align_by_baseline(scope), {
                BasicText!("Hello");
            });
            let l = l.clone();
            let reading =
                Modifier.align_by_baseline(scope).layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let l = l.clone();
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        l.borrow_mut().push(placement.coordinates().is_some());
                        placement.place(&placeable, 0, 0);
                    })
                });
            Box!(modifier = reading, {
                BasicText!("World");
            });
        });
    });
    ui.wait_for_idle();
    let located = located.borrow();
    assert!(located.len() > 1);
    assert!(!located[0]);
    assert!(*located.last().expect("some"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, alignmentLinesTest
#[test]
fn a_ruler_is_its_default_while_aligning_then_there() {
    let ruler = compose_ui::VerticalRuler::new();
    let read = Rc::new(std::cell::RefCell::new(Vec::new()));
    let (r, re) = (ruler.clone(), read.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let (r, re) = (r.clone(), re.clone());
        let r2 = r.clone();
        let providing =
            Modifier.size((dp(100), dp(150))).layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                let r = r2.clone();
                scope.layout_with_rulers(
                    placeable.width,
                    placeable.height,
                    move |rulers| rulers.provides(&r, 35.0),
                    move |placement| placement.place(&placeable, 0, 0),
                )
            });
        Row!(modifier = providing, |scope| {
            let (r, re) = (r.clone(), re.clone());
            let reading =
                Modifier.align_by_baseline(scope).layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let (r, re) = (r.clone(), re.clone());
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        re.borrow_mut().push(placement.current(&r, f32::NAN));
                        placement.place(&placeable, 0, 0);
                    })
                });
            BasicText!("Hello", modifier = reading);
            BasicText!("World");
        });
    });
    ui.wait_for_idle();
    let read = read.borrow();
    assert!(read.len() > 1);
    assert!(read[0].is_nan());
    assert!(!read.last().expect("some").is_nan());
}
