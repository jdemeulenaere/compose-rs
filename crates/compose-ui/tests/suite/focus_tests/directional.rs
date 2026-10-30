//! Moves among parents and children: `Enter`, `Exit`, a directional search leaving a parent and
//! going into one, and the search with nothing focused.

use std::rc::Rc;

use super::support::*;

const T: fn(Scene, &'static str, Bounds) -> Modifier = Scene::target_at;

/// A scene of a parent's `Stack!` at `bounds`.
macro_rules! parent {
    ($modifier:expr, { $($content:tt)* }) => {
        Stack!(modifier = $modifier, { $($content)* });
    };
}

// ---- Enter ----------------------------------------------------------------------------------------

const BIG: Bounds = ltrb(0, 0, 300, 300);
const INNER: Bounds = ltrb(20, 20, 200, 200);
const SMALL: Bounds = ltrb(20, 20, 60, 60);

fn enter(what: &'static str, content: fn(Scene), moved: bool, to: &'static str) -> Move {
    Move { what, content, from: "focused", direction: Enter, moved, to }
}

/// Six children in two rows of three, the first deactivated if asked.
fn two_rows(scene: Scene, modifier: Modifier, first_deactivated: bool) {
    parent!(modifier, {
        for (index, name) in ["a", "b", "c", "d", "e", "f"].into_iter().enumerate() {
            let (column, row) = (index as i32 % 3, index as i32 / 3);
            let place = ltrb(20 + column * 60, 20 + row * 60, 60 + column * 60, 60 + row * 60);
            if index == 0 && first_deactivated {
                Leaf(place.modifier().then(scene.deactivated(name)));
            } else {
                Leaf(T(scene, name, place));
            }
        }
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalEnterTest.kt, moveFocusEnter_noChildren_doesNotMoveFocus, moveFocusEnter_deactivatedChild_doesNotMoveFocus, moveFocusEnter_focusesOnChild, moveFocusEnter_focusesOnImmediateChild, moveFocusEnter_skipsImmediateDeactivatedChild, moveFocusEnter_deactivatedChild_withCustomEnter_canCancelFocus, moveFocusEnter_deactivatedChild_withCustomEnter_canRedirectFocusEnter, moveFocusEnter_topLeftChildIsFocused, moveFocusEnter_deactivatedTopLeftChildIsSkipped, moveFocusEnter_customChildIsFocused
#[test]
fn entering_goes_to_the_nearest_children_that_can_take_the_focus() {
    check_moves(&[
        enter(
            "no children, a sibling beside",
            |scene| {
                Leaf(T(scene, "focused", SMALL));
                Leaf(T(scene, "other", ltrb(60, 20, 100, 60)));
            },
            false,
            "focused",
        ),
        enter(
            "a child that can't take it",
            |scene| {
                parent!(T(scene, "focused", BIG), {
                    Leaf(SMALL.modifier().then(scene.deactivated("child")));
                });
                Leaf(T(scene, "other", ltrb(300, 0, 340, 40)));
            },
            false,
            "focused",
        ),
        enter(
            "a child",
            |scene| {
                parent!(T(scene, "focused", BIG), {
                    Leaf(T(scene, "child", SMALL));
                });
            },
            true,
            "child",
        ),
        enter(
            "a child over a grandchild",
            |scene| {
                parent!(T(scene, "focused", BIG), {
                    parent!(T(scene, "child", INNER), {
                        Leaf(T(scene, "grandchild", SMALL));
                    });
                });
            },
            true,
            "child",
        ),
        enter(
            "through a child that can't take it",
            |scene| {
                parent!(T(scene, "focused", BIG), {
                    parent!(INNER.modifier().then(scene.deactivated("child")), {
                        Leaf(T(scene, "grandchild", SMALL));
                    });
                });
            },
            true,
            "grandchild",
        ),
        enter(
            "a deactivated child's enter cancelling",
            |scene| {
                parent!(T(scene, "focused", BIG), {
                    let cancels =
                        properties(|p| p.set_on_enter(|scope| scope.cancel_focus_change()));
                    parent!(INNER.modifier().then(cancels).then(scene.deactivated("child")), {
                        Leaf(T(scene, "grandchild", SMALL));
                    });
                });
            },
            false,
            "focused",
        ),
        enter(
            "a deactivated child's enter redirecting",
            |scene| {
                parent!(T(scene, "focused", BIG), {
                    let there = scene.requester("second");
                    let redirects = properties(move |p| {
                        p.set_on_enter(move |_| {
                            there.request_focus();
                        })
                    });
                    parent!(INNER.modifier().then(redirects).then(scene.deactivated("child")), {
                        Leaf(T(scene, "first", SMALL));
                        Leaf(T(scene, "second", ltrb(70, 20, 110, 60)));
                    });
                });
            },
            true,
            "second",
        ),
        enter(
            "the child at the top left",
            |scene| two_rows(scene, T(scene, "focused", BIG), false),
            true,
            "a",
        ),
        enter(
            "the one below it if that can't take it: nearer than the next along",
            |scene| two_rows(scene, T(scene, "focused", BIG), true),
            true,
            "d",
        ),
        enter(
            "the parent's own enter choosing",
            |scene| {
                let there = scene.requester("e");
                let chooses = properties(move |p| {
                    p.set_on_enter(move |_| {
                        there.request_focus();
                    })
                });
                two_rows(scene, chooses.then(T(scene, "focused", BIG)), false);
            },
            true,
            "e",
        ),
    ]);
}

// ---- Exit -----------------------------------------------------------------------------------------

fn exit(what: &'static str, content: fn(Scene), moved: bool, to: &'static str) -> Move {
    Move { what, content, from: "focused", direction: Exit, moved, to }
}

/// `focused` in a parent in a grandparent, the parent's and grandparent's deactivated if asked,
/// `on_focused` on the focused one's chain, and another target beside them all.
fn nested(
    scene: Scene,
    parent_deactivated: bool,
    grandparent_deactivated: bool,
    on_focused: fn(Scene) -> Modifier,
) {
    let grandparent = if grandparent_deactivated { Scene::deactivated } else { Scene::target };
    let parent = if parent_deactivated { Scene::deactivated } else { Scene::target };
    parent!(BIG.modifier().then(grandparent(scene, "grandparent")), {
        parent!(INNER.modifier().then(parent(scene, "parent")), {
            Leaf(SMALL.modifier().then(on_focused(scene)).then(scene.target("focused")));
        });
    });
    Leaf(T(scene, "other", ltrb(320, 0, 360, 40)));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalExitTest.kt, moveFocusExit_noParent_focusStateUnchanged, moveFocusExit_focusesOnParent, moveFocusExit_doesNotFocusOnDeactivatedParent, moveFocusExit_focusesOnImmediateParent, moveFocusExit_skipsImmediateParentIfItIsDeactivated, moveFocusExit_deactivatedParentCanCancelExit, moveFocusExit_triggersExitPropertyOnFocusedItem, explicitMoveFocusExit_deactivatedParentCanRedirectExit, moveFocusExit_doesNotChangeIfAllParentsAreDeactivated
#[test]
fn exiting_goes_to_the_nearest_parent_that_can_take_the_focus() {
    check_moves(&[
        exit("no parent", |scene| Leaf(T(scene, "focused", SMALL)), false, "focused"),
        exit(
            "a parent",
            |scene| {
                parent!(T(scene, "parent", BIG), {
                    Leaf(T(scene, "focused", SMALL));
                });
            },
            true,
            "parent",
        ),
        exit(
            "a parent that can't take it",
            |scene| {
                parent!(BIG.modifier().then(scene.deactivated("parent")), {
                    Leaf(T(scene, "focused", SMALL));
                });
            },
            false,
            "focused",
        ),
        exit(
            "the immediate parent",
            |scene| nested(scene, false, false, |_| Modifier),
            true,
            "parent",
        ),
        exit(
            "the grandparent, past a parent that can't",
            |scene| nested(scene, true, false, |_| Modifier),
            true,
            "grandparent",
        ),
        exit("no parent can", |scene| nested(scene, true, true, |_| Modifier), false, "focused"),
        exit(
            "the focused one's exit cancelling, with no parent to go to",
            |scene| {
                nested(scene, true, false, |_| {
                    properties(|p| p.set_on_exit(|scope| scope.cancel_focus_change()))
                });
            },
            false,
            "focused",
        ),
        exit(
            "the focused one's exit redirecting",
            |scene| {
                nested(scene, true, false, |scene| {
                    let there = scene.requester("other");
                    properties(move |p| {
                        p.set_on_exit(move |_| {
                            there.request_focus();
                        })
                    })
                });
            },
            true,
            "other",
        ),
    ]);
}

// ---- Leaving a parent in a direction --------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalExitTest.kt, moveFocusRight_focusesOnSiblingOfParent, moveFocusRight_focusesOnNonDeactivatedSiblingOfParent, moveFocusRight_focusesOnNonDeactivatedSiblingOfParent_withOverlappingDeactivatedItem, moveFocusRight_focusesOnSiblingOfGrandparent
#[test]
fn a_search_leaves_the_parent_for_its_siblings_skipping_those_that_cant_take_the_focus() {
    // In the search's axes: `focused` inside its parents, targets past them on the same line.
    for direction in DIRECTIONS {
        let at = move |major: Span, minor: Span| on_screen(direction, major, minor, false);
        let relative = |outer: Bounds, inner: Bounds| {
            ltrb(
                inner.left - outer.left,
                inner.top - outer.top,
                inner.right - outer.left,
                inner.bottom - outer.top,
            )
        };
        let outer = at((100, 300), (100, 300));
        let focused = relative(outer, at((180, 220), (180, 220)));

        // The sibling of a parent.
        let results = [
            // (what's past the parent, what should have it)
            ("next", Option::<&str>::None, (320, 360), (320, 360)),
            // Past one that can't take it, and over one that can't.
            ("next", Some("deactivated"), (320, 360), (400, 440)),
            ("next", Some("deactivated"), (320, 360), (320, 360)),
        ];
        for (next, deactivated, deactivated_major, next_major) in results {
            let scene = scene(move |scene| {
                Stack!(modifier = outer.modifier().then(scene.target("parent")), {
                    Leaf(focused.modifier().then(scene.target("focused")));
                });
                if let Some(name) = deactivated {
                    Leaf(
                        at(deactivated_major, (180, 220)).modifier().then(scene.deactivated(name)),
                    );
                }
                Leaf(scene.target_at(next, at(next_major, (180, 220))));
            });
            assert!(scene.request("focused"));
            assert!(scene.move_focus(direction));
            assert_eq!(scene.focused(), ["next"], "{direction:?}");
        }

        // The sibling of a grandparent, the parent being inside it.
        let middle = at((120, 280), (120, 280));
        let scene = scene(move |scene| {
            Stack!(modifier = outer.modifier().then(scene.target("grandparent")), {
                Stack!(
                    modifier = relative(outer, middle).modifier().then(scene.target("parent")),
                    {
                        Leaf(
                            relative(middle, at((180, 220), (180, 220)))
                                .modifier()
                                .then(scene.target("focused")),
                        );
                    }
                );
            });
            Leaf(scene.target_at("next", at((320, 360), (180, 220))));
        });
        assert!(scene.request("focused"));
        assert!(scene.move_focus(direction));
        assert_eq!(scene.focused(), ["next"], "{direction:?} past two parents");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalExitTest.kt, moveFocusLeft_fromItemOnLeftEdge_movesFocusOutsideParent, moveFocusLeft_fromItemOnLeftEdge_movesFocusOutsideDeactivatedParent, moveFocusRight_fromItemOnRightEdge_movesFocusOutsideParent, moveFocusRight_fromItemOnRightEdge_movesFocusOutsideDeactivatedParent, moveFocusUp_fromTopmostItem_movesFocusOutsideParent, moveFocusUp_fromTopmostItem_movesFocusOutsideDeactivatedParent, moveFocusDown_fromBottommostItem_movesFocusOutsideParent, moveFocusDown_fromBottommostItem_movesFocusOutsideDeactivatedParent
#[test]
fn from_the_edge_of_its_parent_a_search_goes_to_the_one_outside_on_its_line() {
    for direction in DIRECTIONS {
        for deactivated in [false, true] {
            let at = move |major: Span, minor: Span| on_screen(direction, major, minor, false);
            let parent_bounds = at((100, 200), (100, 400));
            let relative = move |inner: Bounds| {
                ltrb(
                    inner.left - parent_bounds.left,
                    inner.top - parent_bounds.top,
                    inner.right - parent_bounds.left,
                    inner.bottom - parent_bounds.top,
                )
            };
            let scene = scene(move |scene| {
                // The ones outside first, as a search goes by place and not by tree.
                for (name, minor) in
                    [("out1", (120, 160)), ("out2", (220, 260)), ("out3", (320, 360))]
                {
                    Leaf(scene.target_at(name, at((220, 260), minor)));
                }
                let parent = if deactivated { Scene::deactivated } else { Scene::target };
                Stack!(modifier = parent_bounds.modifier().then(parent(scene, "parent")), {
                    Leaf(
                        relative(at((140, 180), (120, 160)))
                            .modifier()
                            .then(scene.target("focused")),
                    );
                    Leaf(relative(at((140, 180), (220, 260))).modifier().then(scene.target("in2")));
                    Leaf(relative(at((140, 180), (320, 360))).modifier().then(scene.target("in3")));
                });
            });
            assert!(scene.request("focused"));
            assert!(scene.move_focus(direction));
            assert_eq!(scene.focused(), ["out1"], "{direction:?}, deactivated {deactivated}");
        }
    }
}

// ---- Searches through a hierarchy, in lines -------------------------------------------------------

/// `content` in a line along `direction`'s axis.
fn along(direction: FocusDirection, content: impl Fn() + 'static) {
    if matches!(direction, Up | Down) {
        Column!({ content() });
    } else {
        Row!({ content() });
    }
}

const ITEM: Bounds = ltrb(0, 0, 40, 40);

/// The start and the end of a line, for a search toward `direction`.
fn ends(direction: FocusDirection) -> (&'static str, &'static str) {
    if matches!(direction, Up | Left) { ("last", "first") } else { ("first", "last") }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalTest.kt, movesFocusAmongSiblingsDeepInTheFocusHierarchy, movesFocusAmongSiblingsDeepInTheFocusHierarchy_skipsDeactivatedSibling
#[test]
fn deep_in_a_hierarchy_a_search_moves_among_siblings_skipping_those_that_cant_take_the_focus() {
    for direction in DIRECTIONS {
        for with_deactivated_between in [false, true] {
            let scene = scene(move |scene| {
                let deep = |content: Rc<dyn Fn()>, level: usize| {
                    fn wrap(depth: usize, content: Rc<dyn Fn()>) {
                        if depth == 0 {
                            content();
                        } else {
                            Stack!(modifier = Modifier.focus_target(), {
                                wrap(depth - 1, content.clone());
                            });
                        }
                    }
                    wrap(level, content);
                };
                deep(
                    Rc::new(move || {
                        along(direction, move || {
                            Leaf(ITEM.modifier().then(scene.target("first")));
                            if with_deactivated_between {
                                Leaf(ITEM.modifier().then(scene.deactivated("between")));
                            }
                            Leaf(ITEM.modifier().then(scene.target("last")));
                        });
                    }),
                    6,
                );
            });
            let (from, to) = ends(direction);
            assert!(scene.request(from));
            assert!(scene.move_focus(direction));
            assert_eq!(scene.focused(), [to], "{direction:?}");
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalTest.kt, movesFocusOutsideCurrentParent, movesOutsideDeactivatedParent
#[test]
fn a_search_leaves_a_parent_to_the_next_on_the_line_whether_it_can_take_the_focus_or_not() {
    for direction in DIRECTIONS {
        for deactivated in [false, true] {
            let scene = scene(move |scene| {
                along(direction, move || {
                    Leaf(ITEM.modifier().then(scene.target("first")));
                    let parent = if deactivated {
                        scene.deactivated("parent")
                    } else {
                        scene.target("parent")
                    };
                    Stack!(modifier = parent, {
                        Leaf(ITEM.modifier().then(scene.target("focused")));
                    });
                    Leaf(ITEM.modifier().then(scene.target("last")));
                });
            });
            assert!(scene.request("focused"));
            assert!(scene.move_focus(direction));
            assert_eq!(scene.focused(), [ends(direction).1], "{direction:?}");
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalTest.kt, skipsChild, DoesNotSkipChildOfDeactivatedItem
#[test]
fn a_search_goes_past_the_child_of_a_target_that_can_take_the_focus_and_into_one_that_cant() {
    for direction in DIRECTIONS {
        // The source's own child isn't next.
        let scene1 = scene(move |scene| {
            along(direction, move || {
                Leaf(ITEM.modifier().then(scene.target("first")));
                Stack!(modifier = ITEM.modifier().then(scene.target("focused")), {
                    Leaf(ITEM.modifier().then(scene.target("child")));
                });
                Leaf(ITEM.modifier().then(scene.target("last")));
            });
        });
        assert!(scene1.request("focused"));
        assert!(scene1.move_focus(direction));
        assert_eq!(scene1.focused(), [ends(direction).1], "{direction:?}");

        // The child of a deactivated one between the ends is.
        let scene2 = scene(move |scene| {
            along(direction, move || {
                Leaf(ITEM.modifier().then(scene.target("first")));
                Stack!(modifier = ITEM.modifier().then(scene.deactivated("between")), {
                    Leaf(ITEM.modifier().then(scene.target("child")));
                });
                Leaf(ITEM.modifier().then(scene.target("last")));
            });
        });
        let (from, _) = ends(direction);
        assert!(scene2.request(from));
        assert!(scene2.move_focus(direction));
        assert_eq!(scene2.focused(), ["child"], "{direction:?}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalTest.kt, movesFocusAmongSiblings_prefersSiblingToAnAncestorThatIsCloser, movesFocusAmongSiblings_prefersSiblingToAnAncestorThatIsCloser_whenParentIsDeactivated
#[test]
fn a_search_prefers_a_sibling_to_a_closer_item_outside_the_parent() {
    for direction in DIRECTIONS {
        for deactivated in [false, true] {
            let at = move |major: Span| on_screen(direction, major, (0, 40), false);
            // On screen these are absolute, in a parent at the same place as the line.
            let outer = at((100, 340));
            let relative = move |inner: Bounds| {
                ltrb(
                    inner.left - outer.left,
                    inner.top - outer.top,
                    inner.right - outer.left,
                    inner.bottom - outer.top,
                )
            };
            let scene = scene(move |scene| {
                // Between the siblings, and nearer.
                Leaf(scene.target_at("behind", at((140, 160))));
                Stack!(
                    modifier = outer.modifier().then(if deactivated {
                        scene.deactivated("parent")
                    } else {
                        scene.target("parent")
                    }),
                    {
                        Leaf(relative(at((100, 120))).modifier().then(scene.target("before")));
                        Leaf(relative(at((180, 220))).modifier().then(scene.target("focused")));
                        Leaf(relative(at((300, 340))).modifier().then(scene.target("after")));
                    }
                );
                Leaf(scene.target_at("ahead", at((240, 260))));
            });
            assert!(scene.request("focused"));
            assert!(scene.move_focus(direction));
            assert_eq!(scene.focused(), ["after"], "{direction:?}, deactivated {deactivated}");
        }
    }
}

// ---- Into a parent, in a direction ----------------------------------------------------------------

/// A plus: arms at the four sides of a parent in the middle. A search from an arm toward the
/// opposite one meets the parent first.
fn the_arm_toward(direction: FocusDirection) -> &'static str {
    match direction {
        Left => "left",
        Right => "right",
        Up => "up",
        Down => "down",
        _ => unreachable!(),
    }
}

fn the_arm_opposite(direction: FocusDirection) -> &'static str {
    match direction {
        Left => "right",
        Right => "left",
        Up => "down",
        Down => "up",
        _ => unreachable!(),
    }
}

/// The arms, those of `without` left out, and in the middle what `center` composes.
fn plus(scene: Scene, without: Option<&'static str>, center: impl Fn()) {
    for (name, place) in [("up", ltrb(90, 0, 110, 20)), ("left", ltrb(0, 90, 20, 110))] {
        if without != Some(name) {
            Leaf(T(scene, name, place));
        }
    }
    center();
    for (name, place) in [("right", ltrb(180, 90, 200, 110)), ("down", ltrb(90, 180, 110, 200))] {
        if without != Some(name) {
            Leaf(T(scene, name, place));
        }
    }
}

const CENTER: Bounds = ltrb(50, 50, 150, 150);

/// A deactivated parent in the middle of `plus`, cancelling a move into it, and telling the
/// direction of what came in.
fn cancelling_parent(scene: Scene, entered: &'static std::cell::Cell<Option<FocusDirection>>) {
    let cancels = properties(move |p| {
        p.set_on_enter(move |scope| {
            entered.set(Some(scope.requested_focus_direction()));
            scope.cancel_focus_change();
        })
    });
    Stack!(modifier = CENTER.modifier().then(cancels).then(scene.deactivated("parent")), {
        Leaf(T(scene, "child", ltrb(40, 40, 60, 60)));
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalImplicitEnterTest.kt, moveFocusEnter_customChildIsFocused
#[test]
fn a_search_into_a_deactivated_parent_lets_its_enter_choose_by_the_direction() {
    for (direction, chosen) in [(Left, "b"), (Up, "c"), (Down, "d"), (Right, "e")] {
        let scene = scene(move |scene| {
            let requesters = ["b", "c", "d", "e"].map(|name| scene.requester(name));
            let chooses = properties(move |p| {
                p.set_on_enter(move |scope| {
                    let index = match scope.requested_focus_direction() {
                        Left => 0,
                        Up => 1,
                        Down => 2,
                        Right => 3,
                        _ => unreachable!(),
                    };
                    requesters[index].request_focus();
                })
            });
            plus(scene, None, || {
                two_rows(
                    scene,
                    CENTER.modifier().then(chooses.clone()).then(scene.deactivated("parent")),
                    false,
                );
            });
        });
        assert!(scene.request(the_arm_opposite(direction)));
        assert!(scene.move_focus(direction));
        assert_eq!(scene.focused(), [chosen], "{direction:?}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalImplicitEnterTest.kt, moveFocus_skipsItemWithCustomEnter, moveFocusEnter_blockFocusChange_appropriateOtherItemIsFocused
#[test]
fn a_search_a_parents_enter_cancels_goes_on_to_the_next_best() {
    // Equal distances go to the first in the tree: `left` before `right`, `up` before `down`.
    for (direction, past_it, with_no_arm_past_it) in
        [(Left, "left", "up"), (Right, "right", "up"), (Up, "left", "left"), (Down, "left", "left")]
    {
        for (without, expected) in
            [(None, past_it), (Some(the_arm_toward(direction)), with_no_arm_past_it)]
        {
            let entered = cell(None);
            let scene = scene(move |scene| {
                plus(scene, without, || cancelling_parent(scene, entered));
            });
            assert!(scene.request(the_arm_opposite(direction)));
            assert!(scene.move_focus(direction));
            assert_eq!(entered.get(), Some(direction));
            assert_eq!(scene.focused(), [expected], "{direction:?} without {without:?}");
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalImplicitEnterTest.kt, focusOnItem_doesNotTriggerEnter
#[test]
fn a_search_landing_on_a_target_doesnt_run_the_targets_own_enter() {
    for direction in DIRECTIONS {
        let scene = scene(move |scene| {
            plus(scene, None, || {
                let cancels = properties(|p| p.set_on_enter(|scope| scope.cancel_focus_change()));
                Leaf(CENTER.modifier().then(cancels).then(scene.target("item")));
            });
            Leaf(T(scene, "other", ltrb(300, 300, 340, 340)));
        });
        assert!(scene.request(the_arm_opposite(direction)));
        assert!(scene.move_focus(direction));
        assert_eq!(scene.focused(), ["item"], "{direction:?}");
    }
}

// ---- Out of a parent, in a direction --------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalImplicitExitTest.kt, implicitExit_notTriggeredWhenFocusLeavesItem
#[test]
fn a_search_leaving_a_target_doesnt_run_the_targets_own_exit() {
    for direction in DIRECTIONS {
        let scene = scene(move |scene| {
            plus(scene, None, || {
                let there = scene.requester("other");
                let redirects = properties(move |p| {
                    p.set_on_exit(move |_| {
                        there.request_focus();
                    })
                });
                Leaf(CENTER.modifier().then(redirects).then(scene.target("focused")));
            });
            Leaf(T(scene, "other", ltrb(300, 300, 340, 340)));
        });
        assert!(scene.request("focused"));
        assert!(scene.move_focus(direction));
        assert_eq!(scene.focused(), [the_arm_toward(direction)], "{direction:?}");
    }
}

/// A focused target in a deactivated parent with `exit` in the middle of `plus`, in a
/// grandparent if asked.
fn exiting_parent(scene: Scene, exit: Modifier, grouped: bool) {
    if grouped {
        Stack!(modifier = ltrb(40, 40, 160, 160).modifier().focus_group(), {
            Stack!(modifier = ltrb(10, 10, 110, 110).modifier().then(exit.clone()), {
                Leaf(T(scene, "focused", ltrb(40, 40, 60, 60)));
            });
        });
    } else {
        Stack!(modifier = CENTER.modifier().then(exit), {
            Leaf(T(scene, "focused", ltrb(40, 40, 60, 60)));
        });
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalImplicitExitTest.kt, implicitExit_deactivatedParentCanRedirectExit
#[test]
fn a_search_leaving_a_deactivated_parent_lets_its_exit_redirect_by_the_direction() {
    for direction in DIRECTIONS {
        let heard = cell(None);
        let scene = scene(move |scene| {
            let there = scene.requester("other");
            let redirects = properties(move |p| {
                p.set_on_exit(move |scope| {
                    heard.set(Some(scope.requested_focus_direction()));
                    there.request_focus();
                })
            });
            plus(scene, None, || {
                let redirects = redirects.clone();
                Stack!(modifier = T(scene, "grandparent", CENTER), {
                    Stack!(
                        modifier = ltrb(10, 10, 90, 90)
                            .modifier()
                            .then(redirects.clone())
                            .then(scene.deactivated("parent")),
                        {
                            Leaf(T(scene, "focused", ltrb(30, 30, 50, 50)));
                        }
                    );
                });
            });
            Leaf(T(scene, "other", ltrb(300, 300, 340, 340)));
        });
        assert!(scene.request("focused"));
        assert!(scene.move_focus(direction));
        assert_eq!(heard.get(), Some(direction));
        assert_eq!(scene.focused(), ["other"], "{direction:?}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalImplicitExitTest.kt, implicitExit_notTriggeredWhenThereIsNoDestination
#[test]
fn a_search_with_nowhere_to_go_doesnt_run_the_parents_exit() {
    for direction in DIRECTIONS {
        let heard = cell(None);
        let scene = scene(move |scene| {
            let there = scene.requester("grandparent");
            let redirects = properties(move |p| {
                p.set_on_exit(move |scope| {
                    heard.set(Some(scope.requested_focus_direction()));
                    there.request_focus();
                })
            });
            Stack!(modifier = T(scene, "grandparent", BIG), {
                Stack!(
                    modifier =
                        INNER.modifier().then(redirects.clone()).then(scene.deactivated("parent")),
                    {
                        Leaf(T(scene, "focused", SMALL));
                    }
                );
            });
        });
        assert!(scene.request("focused"));
        assert!(!scene.move_focus(direction));
        assert_eq!(heard.get(), None);
        assert_eq!(scene.focused(), ["focused"], "{direction:?}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalImplicitExitTest.kt, moveFocusExit_blockFocusChange, moveFocusExit_cancelExit, moveFocusExit_redirectExit, moveFocusExit_multipleParents_cancelExit, moveFocusExit_multipleParents_redirectExit
#[test]
fn a_parents_exit_cancelling_keeps_a_search_in_with_or_without_a_request_for_the_focused_target() {
    for direction in DIRECTIONS {
        for grouped in [false, true] {
            for asks_too in [false, true] {
                for through_group in [false, true] {
                    let scene = scene(move |scene| {
                        let itself = scene.requester("focused");
                        let exit = properties(move |p| {
                            p.set_on_exit(move |scope| {
                                if asks_too {
                                    itself.request_focus();
                                }
                                scope.cancel_focus_change();
                            })
                        });
                        let exit = if through_group {
                            exit.focus_group()
                        } else {
                            exit.then(scene.deactivated("parent"))
                        };
                        Leaf(T(scene, "elsewhere", ltrb(300, 300, 340, 340)));
                        plus(scene, None, || exiting_parent(scene, exit.clone(), grouped));
                    });
                    assert!(scene.request("focused"));
                    assert!(!scene.move_focus(direction), "{direction:?}");
                    assert_eq!(scene.focused(), ["focused"], "{direction:?}");
                }
            }
        }
    }
}

// ---- With nothing focused -------------------------------------------------------------------------

/// A column of rows of `columns` items called by `names`, those of `deactivated` not taking the
/// focus.
fn grid(
    scene: Scene,
    columns: usize,
    names: &'static [&'static str],
    deactivated: &'static [&'static str],
) {
    Column!({
        for line in names.chunks(columns) {
            Row!({
                for &name in line {
                    let target = if deactivated.contains(&name) {
                        scene.deactivated(name)
                    } else {
                        scene.target(name)
                    };
                    Leaf(ITEM.modifier().then(target));
                }
            });
        }
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalInitialFocusTest.kt, initialFocus, initialFocus_DeactivatedItemIsSkipped, initialFocus_whenThereIsOnlyOneFocusable
#[test]
fn with_nothing_focused_a_search_starts_at_the_corner_it_moves_away_from() {
    // (columns, names, deactivated, what `Down` finds, what `Right` finds, what `Up` and `Left` find)
    const CASES: [(usize, &[&str], &[&str], &str, &str, &str); 3] = [
        (2, &["a", "b", "c", "d"], &[], "a", "a", "d"),
        // Past the corner's item: along the row going down, and down its column going right.
        (3, &["a", "b", "c", "d", "e", "f", "g", "h", "i"], &["a", "g"], "b", "d", "i"),
        (1, &["only"], &[], "only", "only", "only"),
    ];
    for (columns, names, deactivated, down, right, backward) in CASES {
        for direction in DIRECTIONS {
            let scene = scene(move |scene| grid(scene, columns, names, deactivated));
            assert!(scene.move_focus(direction));
            let expected = match direction {
                Down => down,
                Right => right,
                _ => backward,
            };
            assert_eq!(scene.focused(), [expected], "{direction:?} in {names:?}");
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalInitialFocusTest.kt, doesNotCrash_whenThereIsNoFocusable, doesNotCrash_whenThereIsOneDeactivatedItem
#[test]
fn with_nothing_that_can_take_the_focus_a_search_answers_no() {
    for direction in DIRECTIONS {
        for content in [(|_| Text!("nothing to focus")) as fn(Scene), |scene| {
            Leaf(ITEM.modifier().then(scene.deactivated("only")))
        }] {
            let scene = scene(content);
            assert!(!scene.move_focus(direction), "{direction:?}");
            assert!(!scene.root_has_focus());
        }
    }
}
