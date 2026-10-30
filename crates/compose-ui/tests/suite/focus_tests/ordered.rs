//! `Next` and `Previous`: one walk through a tree, forward and back, and what's left out of it.

use std::rc::Rc;

use compose_runtime::callback;

use super::support::*;

/// A tree of targets, in the order of composition.
#[derive(Clone, Copy)]
enum Node {
    /// A target.
    T(&'static str),
    /// A target that can't take the focus.
    D(&'static str),
    /// A target with children.
    P(&'static str, &'static [Node]),
    /// A target with children, that can't take the focus itself.
    Dp(&'static str, &'static [Node]),
}

use Node::{D, Dp, P, T};

/// Later siblings to the left and above of earlier ones: a walk doesn't go by place.
fn against_the_walk(index: usize) -> Modifier {
    let at = 300.0 - 20.0 * index as f32;
    Modifier.offset(at.dp(), at.dp()).required_size((10.dp(), 10.dp()))
}

fn compose(scene: Scene, nodes: &'static [Node]) {
    for (index, node) in nodes.iter().copied().enumerate() {
        let place = against_the_walk(index);
        match node {
            T(name) => Leaf(place.then(scene.target(name))),
            D(name) => Leaf(place.then(scene.deactivated(name))),
            P(name, children) => {
                Stack!(modifier = place.then(scene.target(name)), {
                    compose(scene, children);
                });
            }
            Dp(name, children) => {
                Stack!(modifier = place.then(scene.deactivated(name)), {
                    compose(scene, children);
                });
            }
        }
    }
}

/// From nothing focused, `Next` goes through `order` and back to its start, and `Previous` the
/// other way.
fn check_walk(tree: &'static [Node], order: &'static [&'static str]) {
    let scene = scene(move |scene| compose(scene, tree));
    for &expected in order.iter().chain(&order[..1]) {
        assert!(scene.move_focus(Next));
        assert_eq!(scene.focused(), [expected], "forward in {order:?}");
    }
    let scene = self::scene(move |scene| compose(scene, tree));
    for &expected in order.iter().rev().chain(order.last()) {
        assert!(scene.move_focus(Previous));
        assert_eq!(scene.focused(), [expected], "back in {order:?}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OneDimensionalFocusSearchNextTest.kt, initialFocus_oneItem, initialFocus_skipsDeactivatedItem, initialFocus_firstItemInCompositionOrderGetsFocus, focusMovesToSecondItem, focusMovesToThirdItem, focusMovesToThirdItem_skipsDeactivatedItem, focusMovesToFourthItem, focusWrapsAroundToFirstItem, focusWrapsAroundToFirstItem_skippingLastDeactivatedItem, focusWrapsAroundToFirstItem_skippingFirstDeactivatedItem
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OneDimensionalFocusSearchPreviousTest.kt, initialFocus_oneItem, initialFocus_skipsDeactivatedItem, initialFocus_lastItemInCompositionOrderGetsFocus, focusMovesToSecondItem, focusMovesToSecondItem_skipsDeactivatedItem, focusMovesToFirstItem, focusMovesToFirstItem_ignoresDeactivated, focusWrapsAroundToLastItem, focusWrapsAroundToLastItem_skippingFirstDeactivatedItem, focusWrapsAroundToLastItem_skippingLastDeactivatedItem
#[test]
fn a_walk_follows_siblings_in_composition_order_and_wraps_past_those_that_cant_take_the_focus() {
    check_walk(&[T("only")], &["only"]);
    check_walk(&[D("a"), T("b")], &["b"]);
    check_walk(&[T("a"), D("b")], &["a"]);
    check_walk(&[T("a"), T("b"), T("c")], &["a", "b", "c"]);
    check_walk(&[T("a"), D("b"), T("c"), T("d")], &["a", "c", "d"]);
    check_walk(&[T("a"), T("b"), T("c"), D("d")], &["a", "b", "c"]);
    check_walk(&[D("a"), T("b"), T("c"), T("d")], &["b", "c", "d"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OneDimensionalFocusSearchNextTest.kt, initialFocus_firstParentInCompositionOrderGetsFocus, initialFocus_firstItemInCompositionOrderGetsFocus_evenIfAnotherNonParentIsPresent, initialFocus_firstItemInCompositionOrderGetsFocus_evenIfThereIsAParentAtTheRoot
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OneDimensionalFocusSearchPreviousTest.kt, initialFocus_lastChildInCompositionOrderGetsFocus, initialFocus_lastItemInCompositionOrderGetsFocus_evenIfAnotherNonParentIsPresent, initialFocus_lastItemInCompositionOrderGetsFocus_evenIfThereIsAParentAtTheRoot, focusMovesToParent
#[test]
fn a_walk_visits_a_parent_before_its_children_and_after_them_going_back() {
    check_walk(&[P("p1", &[T("c1")]), P("p2", &[T("c2")])], &["p1", "c1", "p2", "c2"]);
    check_walk(&[P("p", &[T("c")]), T("item")], &["p", "c", "item"]);
    check_walk(&[T("item"), P("p", &[T("c")])], &["item", "p", "c"]);
    check_walk(&[P("p", &[T("c"), T("item")])], &["p", "c", "item"]);
    check_walk(&[P("p", &[T("c1"), T("c2"), T("c3")])], &["p", "c1", "c2", "c3"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OneDimensionalFocusSearchNextTest.kt, focusMovesToChildOfDeactivatedItem, focusMovesToGrandChildOfDeactivatedItem, focusMovesToNextSiblingOfDeactivatedItem_evenThoughThereIsACloserNonSibling
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OneDimensionalFocusSearchPreviousTest.kt, focusMovesToChildOfDeactivatedItem, focusMovesToGrandChildOfDeactivatedItem, focusMovesToNextSiblingOfDeactivatedItem_evenThoughThereIsACloserNonSibling, focusMovesToParent_ignoresDeactivated, focusMovesToParent_ignoresDeactivated_andWrapsAround
#[test]
fn a_walk_enters_a_deactivated_target_without_stopping_on_it() {
    check_walk(&[T("a"), Dp("b", &[T("c")]), T("d")], &["a", "c", "d"]);
    check_walk(&[T("a"), Dp("b", &[Dp("c", &[T("e")])]), T("d")], &["a", "e", "d"]);
    check_walk(&[T("a"), Dp("b", &[T("c1"), T("c2")]), T("d")], &["a", "c1", "c2", "d"]);
    check_walk(&[T("item"), Dp("p", &[T("c1"), T("c2")])], &["item", "c1", "c2"]);
    check_walk(&[Dp("p", &[T("c1"), T("c2"), T("c3")])], &["c1", "c2", "c3"]);
}

const TWELVE: &[Node] = &[
    P("row1", &[T("a"), T("b"), T("c"), T("d")]),
    P("row2", &[T("e"), T("f"), T("g"), T("h")]),
    P("row3", &[T("i"), T("j"), T("k"), T("l")]),
];

const LEVELS: &[Node] = &[
    T("a"),
    T("b"),
    Dp(
        "row",
        &[
            T("c"),
            T("d"),
            Dp("column", &[T("e"), T("f"), Dp("inner", &[T("g"), T("h")]), T("i"), T("j")]),
            T("k"),
            T("l"),
        ],
    ),
    T("m"),
    T("n"),
];

const MIXED: &[Node] = &[
    P("p1", &[T("c1"), T("c2"), T("c3")]),
    P("p2", &[T("c4"), P("p3", &[T("c6")]), T("c5")]),
    Dp("p4", &[D("c7"), T("c8"), Dp("p5", &[T("c11")]), T("c9"), D("c10")]),
];

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OneDimensionalFocusSearchNextTest.kt, focusNextOrderAmongChildrenOfMultipleParents, focusNextOrderAmongChildrenAtMultipleLevels, focusNextOrdering
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OneDimensionalFocusSearchPreviousTest.kt, focusNextOrderAmongChildrenOfMultipleParents, focusNextOrderAmongChildrenAtMultipleLevels, focusPreviousOrdering
#[test]
fn a_walk_goes_through_a_whole_tree_depth_first() {
    check_walk(
        TWELVE,
        &["row1", "a", "b", "c", "d", "row2", "e", "f", "g", "h", "row3", "i", "j", "k", "l"],
    );
    check_walk(LEVELS, &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n"]);
    check_walk(MIXED, &["p1", "c1", "c2", "c3", "p2", "c4", "p3", "c6", "c5", "c8", "c11", "c9"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OneDimensionalFocusSearchNextTest.kt, moveFocus_noFocusableItem, moveFocus_oneDisabledFocusableItem
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OneDimensionalFocusSearchPreviousTest.kt, moveFocus_noFocusableItem, moveFocus_oneDisabledFocusableItem
#[test]
fn a_walk_with_nothing_to_visit_answers_no() {
    const NOTHING: [&[Node]; 2] = [&[], &[D("a")]];
    for tree in NOTHING {
        for direction in [Next, Previous] {
            let scene = scene(move |scene| compose(scene, tree));
            assert!(!scene.move_focus(direction));
            assert!(!scene.root_has_focus());
        }
    }
}

/// Places its children from the last to the first.
struct LastFirst;

impl MeasurePolicy for LastFirst {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let placeables: Vec<_> =
            measurables.iter().map(|&each| scope.measure(each, constraints)).collect();
        for placeable in placeables.iter().rev() {
            scope.place(placeable, 0, 0);
        }
        scope.layout(100, 100)
    }
}

/// The order is of placing, as Compose's comparison of its children, where a layout says one.
#[test]
fn a_walk_follows_the_order_a_layout_places_in() {
    let scene = scene(|scene| {
        Layout(
            Modifier,
            Rc::new(LastFirst),
            callback(move || {
                for name in ["a", "b", "c"] {
                    Leaf(scene.target(name));
                }
            }),
        );
    });
    for expected in ["c", "b", "a"] {
        assert!(scene.move_focus(Next));
        assert_eq!(scene.focused(), [expected]);
    }
}

// ---- Wrapping or not -------------------------------------------------------------------------------

/// Each arm of the wrap flag, in each input mode.
fn in_each_arm(check: impl Fn(InputMode, bool)) {
    for mode in [InputMode::Touch, InputMode::Keyboard] {
        for wrap in [true, false] {
            check(mode, wrap);
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/WrapAroundFocusTest.kt, noFocusableItem_next, noFocusableItem_previous, singleItem_next, singleItem_previous, singleRow_next, singleRow_previous, twoRows_next_movesToSecondRow, twoRows_previous_movesToFirstRow, twoRows_next_wrapsAround, twoRows_previous_wrapsAround
#[test]
fn a_walk_wraps_at_the_ends_only_if_asked_in_either_input_mode() {
    // (rows of names, from, direction, to with wrapping, to without)
    const CASES: [(&[&[&str]], &str, FocusDirection, &str, &str); 8] = [
        (&[&["a"]], "a", Next, "a", "a"),
        (&[&["a"]], "a", Previous, "a", "a"),
        (&[&["a", "b"]], "b", Next, "a", "b"),
        (&[&["a", "b"]], "a", Previous, "b", "a"),
        (&[&["a", "b"], &["c", "d"]], "b", Next, "c", "c"),
        (&[&["a", "b"], &["c", "d"]], "c", Previous, "b", "b"),
        (&[&["a", "b"], &["c", "d"]], "d", Next, "a", "d"),
        (&[&["a", "b"], &["c", "d"]], "a", Previous, "d", "a"),
    ];
    in_each_arm(|mode, wrap| {
        for (rows, from, direction, wrapped, unwrapped) in CASES {
            let scene = scene(move |scene| {
                Column!({
                    for &row in rows {
                        Row!({
                            for &name in row {
                                Leaf(Modifier.size(10.dp()).then(scene.named(name)).focusable());
                            }
                        });
                    }
                });
            });
            scene.with_host(|host| host.set_input_mode(mode));
            assert!(scene.request(from));
            scene.move_focus_wrapping(direction, wrap);
            let expected = if wrap { wrapped } else { unwrapped };
            assert_eq!(
                scene.focused(),
                [expected],
                "{mode:?} wrap {wrap} {direction:?} from {from}"
            );
        }
        // Nothing to go to, and it doesn't fail.
        for direction in [Next, Previous] {
            let scene = scene(|_| Leaf(Modifier.size(10.dp())));
            scene.with_host(|host| host.set_input_mode(mode));
            assert!(!scene.move_focus_wrapping(direction, wrap));
            assert!(!scene.root_has_focus());
        }
    });
}

// ---- What isn't placed -------------------------------------------------------------------------------

/// Children one after the other, on a line, those of `unplaced` measured and left out.
struct Line {
    horizontal: bool,
    unplaced: &'static [usize],
}

impl MeasurePolicy for Line {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let mut along = 0;
        for (index, &measurable) in measurables.iter().enumerate() {
            let placeable = scope.measure(measurable, constraints);
            if !self.unplaced.contains(&index) {
                let (x, y) = if self.horizontal { (along, 0) } else { (0, along) };
                scope.place(&placeable, x, y);
            }
            along += if self.horizontal { placeable.width } else { placeable.height };
        }
        scope.layout(
            if self.horizontal { along } else { 10 },
            if self.horizontal { 10 } else { along },
        )
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusSearchNonPlacedItemsTest.kt, moveFocusPrevious_skipsUnplacedItem, moveFocusNext_skipsUnplacedItem, moveFocusLeft_skipsUnplacedItem, moveFocusRight_skipsUnplacedItem, moveFocusUp_skipsUnplacedItem, moveFocusDown_skipsUnplacedItem, moveFocusEnter_skipsUnplacedItem, moveFocusPrevious_skipsFirstUnplacedItem, moveFocusNext_skipsLastUnplacedItem, moveFocusPrevious_skipsMultipleUnplacedItems, moveFocusNext_skipsMultipleUnplacedItems, moveFocusLeft_skipsMultipleUnplacedItems, moveFocusRight_skipsMultipleUnplacedItems, moveFocusUp_skipsMultipleUnplacedItems, moveFocusDown_skipsMultipleUnplacedItems
#[test]
fn a_search_skips_a_target_its_parent_doesnt_place() {
    // (the children, those unplaced, from, direction, to)
    const CASES: [(&[&str], &[usize], &str, FocusDirection, &str); 15] = [
        (&["a", "b", "c"], &[1], "c", Previous, "a"),
        (&["a", "b", "c"], &[1], "a", Next, "c"),
        (&["a", "b", "c"], &[1], "c", Left, "a"),
        (&["a", "b", "c"], &[1], "a", Right, "c"),
        (&["a", "b", "c"], &[1], "c", Up, "a"),
        (&["a", "b", "c"], &[1], "a", Down, "c"),
        (&["a", "b"], &[0], "parent", Enter, "b"),
        // The first and the last not placed: the parent is next, in the walk.
        (&["a", "b", "c"], &[0], "b", Previous, "parent"),
        (&["a", "b", "c"], &[2], "b", Next, "parent"),
        (&["a", "b", "c", "d"], &[1, 2], "d", Previous, "a"),
        (&["a", "b", "c", "d"], &[1, 2], "a", Next, "d"),
        (&["a", "b", "c", "d"], &[1, 2], "d", Left, "a"),
        (&["a", "b", "c", "d"], &[1, 2], "a", Right, "d"),
        (&["a", "b", "c", "d"], &[1, 2], "d", Up, "a"),
        (&["a", "b", "c", "d"], &[1, 2], "a", Down, "d"),
    ];
    for (names, unplaced, from, direction, to) in CASES {
        let horizontal = !matches!(direction, Up | Down);
        let scene = scene(move |scene| {
            Layout(
                scene.target("parent"),
                Rc::new(Line { horizontal, unplaced }),
                callback(move || {
                    for &name in names {
                        Leaf(Modifier.size(10.dp()).then(scene.target(name)));
                    }
                }),
            );
        });
        assert!(scene.request(from));
        assert!(scene.move_focus(direction));
        assert_eq!(scene.focused(), [to], "{direction:?} from {from}, {unplaced:?} unplaced");
    }
}
