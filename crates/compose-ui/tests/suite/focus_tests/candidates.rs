//! Which target a directional search picks among siblings, in every direction: the rules are
//! written once, in the search's own axes, and each row is turned into the four directions.

use super::support::*;

const DIRECTIONS: [FocusDirection; 4] = [Left, Right, Up, Down];

/// The items of a table's row, in order: the first has the focus. Composed in `order`, as a
/// sibling's place in the tree says nothing to the search.
fn search_among(
    direction: FocusDirection,
    items: Vec<(&'static str, Bounds)>,
) -> Vec<&'static str> {
    let source = items[0].0;
    let scene = scene(move |scene| {
        for &(name, place) in &items {
            Leaf(scene.target_at(name, place));
        }
    });
    assert!(scene.request(source));
    scene.move_focus(direction);
    scene.focused()
}

// One interval against the source's (100, 200) on the search's axis: is it a move toward the lower
// end, a move toward the higher end, or neither? A rectangle counts as ahead only if both its edges
// are past the source's matching edges.
const INTERVALS: [(Span, bool, bool); 13] = [
    // Clear of the source, then touching it.
    ((20, 60), true, false),
    ((60, 100), true, false),
    // Across its lower edge.
    ((60, 140), true, false),
    // Starting at its lower edge: not past it on the lower side, not past its far edge.
    ((100, 140), false, false),
    // Inside it.
    ((120, 180), false, false),
    // Ending at its far edge.
    ((140, 200), false, false),
    // The same.
    ((100, 200), false, false),
    // Containing it, sharing an edge or not.
    ((100, 240), false, false),
    ((60, 240), false, false),
    ((60, 200), false, false),
    // Across its far edge, touching it, clear of it.
    ((140, 240), false, true),
    ((200, 240), false, true),
    ((240, 280), false, true),
];

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalTwoItemsTest.kt, nonOverlappingCandidate*, overlappingCandidate*, candidateWithCommonBoundary*, candidateWithinBoundsOfFocusedItem*, focusedItemWithinBoundsOfCandidate*, sameBoundsForFocusedItemAndCandidate
#[test]
fn a_lone_sibling_is_a_candidate_if_it_lies_past_the_source_whatever_its_place_across() {
    let source: Span = (100, 200);
    // A direction's axes turned to the screen's: `true` when it grows with the coordinate.
    for (direction, increasing, horizontal) in
        [(Left, false, true), (Right, true, true), (Up, false, false), (Down, true, false)]
    {
        for (along, toward_lower, toward_higher) in INTERVALS {
            for (across, _, _) in INTERVALS {
                let (x, y) = if horizontal { (along, across) } else { (across, along) };
                let candidate = ltrb(x.0, y.0, x.1, y.1);
                let moves = if increasing { toward_higher } else { toward_lower };
                let focused = search_among(
                    direction,
                    vec![
                        ("source", ltrb(source.0, source.0, source.1, source.1)),
                        ("candidate", candidate),
                    ],
                );
                let expected = if moves { "candidate" } else { "source" };
                assert_eq!(focused, [expected], "{direction:?} to {candidate:?}");
            }
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalTwoItemsTest.kt, picksSiblingAndNotChild
#[test]
fn a_search_picks_the_sibling_over_the_child_of_the_source() {
    for direction in DIRECTIONS {
        let scene = scene(move |scene| {
            Stack!(
                modifier =
                    scene.target_at("source", on_screen(direction, (100, 200), (100, 200), false)),
                {
                    // Inside the source: the search doesn't enter what it leaves.
                    Leaf(scene.target_at("child", ltrb(40, 40, 60, 60)));
                }
            );
            Leaf(scene.target_at("sibling", on_screen(direction, (220, 240), (140, 160), false)));
        });
        assert!(scene.request("source"));
        assert!(scene.move_focus(direction));
        assert_eq!(scene.focused(), ["sibling"], "{direction:?}");
    }
}

/// `source` is 100 long on each axis, from 100; each row's first item is what should win when
/// searched for along the axis the row says.
struct Rule {
    what: &'static str,
    first: (Span, Span),
    second: (Span, Span),
    /// Whether the first wins on the horizontal axis and on the vertical.
    first_wins: (bool, bool),
}

const SOURCE: (Span, Span) = ((100, 120), (100, 120));

// Where in-beam items sit across the source's minor axis, (100, 120): the same, across each of its
// edges, inside it, and around it. The rules hold for each.
const IN_BEAM: [Span; 5] = [(100, 120), (90, 110), (110, 130), (105, 115), (90, 130)];

// Where items out of the beam sit across it: touching it, a little away, far away.
const OUT_OF_BEAM: [Span; 3] = [(120, 140), (130, 150), (300, 320)];

fn check(rule: &Rule) {
    for direction in DIRECTIONS {
        for mirrored in [false, true] {
            let place = |(major, minor): (Span, Span)| on_screen(direction, major, minor, mirrored);
            let horizontal = matches!(direction, Left | Right);
            let first_wins = if horizontal { rule.first_wins.0 } else { rule.first_wins.1 };
            let expected = if first_wins { "first" } else { "second" };
            for order in [false, true] {
                let mut items = vec![("first", place(rule.first)), ("second", place(rule.second))];
                if order {
                    items.reverse();
                }
                items.insert(0, ("source", place(SOURCE)));
                assert_eq!(
                    search_among(direction, items),
                    [expected],
                    "{}: {direction:?} mirrored {mirrored}",
                    rule.what
                );
            }
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalThreeItemsTest.kt, validItemIsPickedEvenThoughThereIsACloserItem*
#[test]
fn a_candidate_past_the_source_wins_over_a_closer_item_that_isnt_past_it() {
    for minor in OUT_OF_BEAM.into_iter().chain(IN_BEAM) {
        for closer_minor in OUT_OF_BEAM {
            check(&Rule {
                what: "valid over closer",
                first: ((200, 220), minor),
                // Reaching behind the source's near edge.
                second: ((90, 110), closer_minor),
                first_wins: (true, true),
            });
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalThreeItemsTest.kt, closerItemWinsForItemsOutsideBeam*
#[test]
fn the_closer_of_two_items_outside_the_beam_wins() {
    for far in OUT_OF_BEAM {
        for near in OUT_OF_BEAM {
            if far.0 >= near.0 {
                check(&Rule {
                    what: "outside the beam",
                    first: ((130, 150), near),
                    second: ((400, 420), far),
                    first_wins: (true, true),
                });
            }
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalThreeItemsTest.kt, closerItemWinsWhenThereAreMultipleItemsInBeam*
#[test]
fn the_closer_of_two_items_in_the_beam_wins() {
    for first in IN_BEAM {
        for second in IN_BEAM {
            check(&Rule {
                what: "in the beam",
                first: ((130, 150), first),
                second: ((200, 220), second),
                first_wins: (true, true),
            });
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalThreeItemsTest.kt, fartherItemWinsWhenTheMinorAxisDistanceIsMuchSmaller
#[test]
fn outside_the_beam_a_small_distance_across_outweighs_a_larger_one_along() {
    check(&Rule {
        what: "far across",
        first: ((140, 160), (130, 150)),
        second: ((130, 150), (300, 320)),
        first_wins: (true, true),
    });
    // The distance along counts for more: this one is nearer along, and less near across.
    check(&Rule {
        what: "along weighs more",
        first: ((130, 150), (150, 170)),
        second: ((140, 160), (135, 155)),
        first_wins: (true, true),
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalThreeItemsTest.kt, inBeamOverlappingItemWinsOverCloserItemWithOverlappingMajorAxis*, inBeamWinsOverCloserItemWithOverlappingMajorAxis*
#[test]
fn an_item_in_the_beam_wins_over_a_closer_one_that_overlaps_the_source_along_the_search() {
    for in_beam_major in [(300, 320), (110, 130)] {
        for in_beam in IN_BEAM {
            for closer in OUT_OF_BEAM {
                check(&Rule {
                    what: "in beam over overlapping along",
                    first: (in_beam_major, in_beam),
                    second: ((110, 130), closer),
                    first_wins: (true, true),
                });
            }
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalThreeItemsTest.kt, inBeamWinsOverCloserItemForHorizontalSearchButNotForVerticalSearch*
#[test]
fn an_item_in_the_beam_wins_when_searching_across_lines_but_a_closer_line_wins_going_down_lines() {
    for in_beam in IN_BEAM {
        for closer in OUT_OF_BEAM {
            check(&Rule {
                what: "in beam horizontally only",
                // Wholly past what the other reaches.
                first: ((200, 220), in_beam),
                second: ((125, 135), closer),
                first_wins: (true, false),
            });
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalThreeItemsTest.kt, inBeamWinsOverOtherItemWithSameFarEdge*, inBeamWinsOverOtherItemWithFarEdgeGreaterThanInBeamCloserEdge*, inBeamWinsOverOtherItemWithFarEdgeEqualToInBeamCloserEdge_forHorizontalSearch*
#[test]
fn an_item_in_the_beam_wins_over_one_reaching_at_least_as_far() {
    for in_beam in IN_BEAM {
        for closer in OUT_OF_BEAM {
            for (reach, vertical_too) in [
                // The other's far edge is past where the item in the beam starts: both axes.
                ((125, 220), true),
                ((125, 240), true),
                // It ends just where the item in the beam starts: a line above, going down.
                ((125, 200), false),
            ] {
                check(&Rule {
                    what: "reach",
                    first: ((200, 220), in_beam),
                    second: (reach, closer),
                    first_wins: (true, vertical_too),
                });
            }
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalThreeItemsTest.kt, inBeamWinsOverOtherItemWithSameMajorAxisDistance*
#[test]
fn an_item_in_the_beam_wins_over_one_as_far_along_the_search() {
    for in_beam in IN_BEAM {
        for closer in OUT_OF_BEAM {
            check(&Rule {
                what: "same distance",
                first: ((200, 220), in_beam),
                second: ((200, 220), closer),
                first_wins: (true, true),
            });
        }
    }
}
