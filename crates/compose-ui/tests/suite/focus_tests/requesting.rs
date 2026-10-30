//! `FocusRequester`: requesting, capturing and freeing, from each state and in each relation,
//! and which target a requester stands for.

use super::support::*;

// ---- A request, from each state --------------------------------------------------------------------

/// What happens to a request: `first` asked for, captured if asked, then `request` asked for.
struct Request {
    what: &'static str,
    content: fn(Scene),
    first: Option<&'static str>,
    capture: bool,
    request: &'static str,
    states: &'static [(&'static str, FocusState)],
}

fn check_requests(rows: &[Request]) {
    for row in rows {
        let scene = scene(row.content);
        if let Some(first) = row.first {
            scene.request(first);
            if row.capture {
                assert!(scene.capture(first), "{}: capturing", row.what);
            }
        }
        scene.request(row.request);
        for &(name, state) in row.states {
            assert_eq!(scene.state(name), state, "{}: {name}", row.what);
        }
    }
}

const fn asked(
    what: &'static str,
    content: fn(Scene),
    first: Option<&'static str>,
    capture: bool,
    request: &'static str,
    states: &'static [(&'static str, FocusState)],
) -> Request {
    Request { what, content, first, capture, request, states }
}

fn one(scene: Scene) {
    Leaf(scene.target("x"));
}

fn parent_child(scene: Scene) {
    Stack!(modifier = scene.target("parent"), {
        Leaf(scene.target("child"));
    });
}

fn deactivated_parent_child(scene: Scene) {
    Stack!(modifier = scene.deactivated("parent"), {
        Leaf(scene.target("child"));
    });
}

fn siblings(scene: Scene) {
    Stack!(modifier = scene.target("parent"), {
        Leaf(scene.target("x"));
        Leaf(scene.target("sibling"));
    });
}

fn three_levels(scene: Scene) {
    Stack!(modifier = scene.target("top"), {
        Stack!(modifier = scene.target("middle"), {
            Leaf(scene.target("bottom"));
        });
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusTest.kt, active_isUnchanged, captured_isUnchanged, deactivated_isUnchanged, activeParent_propagateFocus, deactivatedParent_propagateFocus, deactivatedParent_activeChild_propagateFocus, inactiveRoot_propagateFocusSendsRequestToOwner_systemCanGrantFocus, inactiveRootWithChildren_propagateFocusSendsRequestToOwner_systemCanGrantFocus, inactiveNonRootWithChildren, rootNode, rootNodeWithChildren, parentNodeWithNoFocusedAncestor, parentNodeWithNoFocusedAncestor_childRequestsFocus, childNodeWithNoFocusedAncestor, requestFocus_parentIsFocused, requestFocus_childIsFocused, requestFocus_childHasCapturedFocus, requestFocus_siblingIsFocused, requestFocus_siblingHasCapturedFocused, requestFocus_cousinIsFocused, requestFocus_grandParentIsFocused
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedTest.kt, active_requestFocus, activeParent_requestFocus, captured_requestFocus, deactivated_requestFocus, deactivatedParent_requestFocus, inactive_requestFocus
#[test]
fn a_request_from_each_state_and_each_relation_leaves_the_states_it_should() {
    check_requests(&[
        asked("already active", one, Some("x"), false, "x", &[("x", Active)]),
        asked("captured", one, Some("x"), true, "x", &[("x", Captured)]),
        asked(
            "can't take it",
            |scene| Leaf(scene.deactivated("x")),
            None,
            false,
            "x",
            &[("x", Inactive)],
        ),
        asked(
            "a parent over its focused child",
            parent_child,
            Some("child"),
            false,
            "parent",
            &[("parent", Active), ("child", Inactive)],
        ),
        asked(
            "a deactivated parent over its focused child",
            deactivated_parent_child,
            Some("child"),
            false,
            "parent",
            &[("parent", ActiveParent), ("child", Active)],
        ),
        asked(
            "a deactivated parent over a focused child with children of its own",
            |scene| {
                Stack!(modifier = scene.deactivated("parent"), {
                    Stack!(modifier = scene.target("child"), {
                        Leaf(scene.target("grandchild"));
                    });
                });
            },
            Some("child"),
            false,
            "parent",
            &[("parent", ActiveParent), ("child", Active), ("grandchild", Inactive)],
        ),
        asked("nothing focused", one, None, false, "x", &[("x", Active)]),
        asked(
            "nothing focused, with a child",
            parent_child,
            None,
            false,
            "parent",
            &[("parent", Active), ("child", Inactive)],
        ),
        asked(
            "under a focused one, with a child",
            three_levels,
            Some("top"),
            false,
            "middle",
            &[("top", ActiveParent), ("middle", Active), ("bottom", Inactive)],
        ),
        asked(
            "between unfocused ones",
            three_levels,
            None,
            false,
            "middle",
            &[("top", ActiveParent), ("middle", Active), ("bottom", Inactive)],
        ),
        asked(
            "a child, its parents following",
            three_levels,
            None,
            false,
            "bottom",
            &[("top", ActiveParent), ("middle", ActiveParent), ("bottom", Active)],
        ),
        asked(
            "a child of a focused one",
            parent_child,
            Some("parent"),
            false,
            "child",
            &[("parent", ActiveParent), ("child", Active)],
        ),
        asked(
            "a parent while its child has captured it: refused",
            parent_child,
            Some("child"),
            true,
            "parent",
            &[("parent", ActiveParent), ("child", Captured)],
        ),
        asked(
            "a child of a target that has captured it: refused",
            parent_child,
            Some("parent"),
            true,
            "child",
            &[("parent", Captured), ("child", Inactive)],
        ),
        asked(
            "a sibling of a focused one",
            siblings,
            Some("sibling"),
            false,
            "x",
            &[("parent", ActiveParent), ("x", Active), ("sibling", Inactive)],
        ),
        asked(
            "a sibling of one that has captured it: refused",
            siblings,
            Some("sibling"),
            true,
            "x",
            &[("parent", ActiveParent), ("x", Inactive), ("sibling", Captured)],
        ),
        asked(
            "a cousin of a focused one",
            |scene| {
                Stack!(modifier = Modifier.focus_target(), {
                    Stack!(modifier = Modifier.focus_target(), {
                        Leaf(scene.target("x"));
                    });
                    Stack!(modifier = Modifier.focus_target(), {
                        Leaf(scene.target("cousin"));
                    });
                });
            },
            Some("cousin"),
            false,
            "x",
            &[("x", Active), ("cousin", Inactive)],
        ),
        asked(
            "two levels under a focused one",
            three_levels,
            Some("top"),
            false,
            "bottom",
            &[("top", ActiveParent), ("middle", ActiveParent), ("bottom", Active)],
        ),
    ]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedTest.kt, inactive_requestFocus_multipleObservers
#[test]
fn every_listener_above_a_target_hears_it_through_layouts_without_targets() {
    let scene = scene(|scene| {
        Stack!(modifier = scene.listen("a").then(scene.listen("b")), {
            Stack!({
                Stack!(modifier = scene.listen("c").then(scene.listen("d")), {
                    Leaf(
                        scene
                            .listen("e")
                            .then(scene.listen("f"))
                            .then(scene.req("x"))
                            .focus_target(),
                    );
                });
            });
        });
    });
    assert!(scene.request("x"));
    assert_eq!(scene.states(["a", "b", "c", "d", "e", "f"]), [Active; 6]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedTest.kt, active_requestFocus_multipleObserversWithExtraFocusTargetInBetween
#[test]
fn a_listener_hears_the_next_target_of_its_chain_not_the_last() {
    let scene = scene(|scene| {
        Leaf(
            scene
                .listen("outer1")
                .then(scene.listen("outer2"))
                .focus_target()
                .then(scene.listen("inner1"))
                .then(scene.listen("inner2"))
                .then(scene.req("x"))
                .focus_target(),
        );
    });
    assert!(scene.request("x"));
    assert_eq!(scene.states(["outer1", "outer2"]), [ActiveParent; 2]);
    assert_eq!(scene.states(["inner1", "inner2"]), [Active; 2]);
}

/// A target whose changes go to the log, under `name`.
fn logged(scene: Scene, name: &'static str) -> Modifier {
    Modifier.on_focus_changed(move |state: FocusState| scene.say(format!("{name} {state:?}")))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusTest.kt, requestFocus_eventSequence, requestFocusOnParentAndThenChild_eventSequence
#[test]
fn a_request_tells_those_it_leaves_from_the_bottom_up_then_those_it_enters_from_the_top_down() {
    let scene = scene(|scene| {
        Stack!(modifier = logged(scene, "top").focus_target(), {
            Stack!(modifier = logged(scene, "left").focus_target(), {
                Leaf(logged(scene, "one").then(scene.req("one")).focus_target());
            });
            Stack!(modifier = logged(scene, "right").focus_target(), {
                Leaf(logged(scene, "two").then(scene.req("two")).focus_target());
            });
        });
    });
    scene.forget();
    scene.request("one");
    assert_eq!(scene.said(), ["top ActiveParent", "left ActiveParent", "one Active"]);
    scene.forget();
    scene.request("two");
    assert_eq!(scene.said(), ["one Inactive", "left Inactive", "right ActiveParent", "two Active"]);

    let scene = self::scene(|scene| {
        Stack!(modifier = logged(scene, "parent").then(scene.req("parent")).focus_target(), {
            Leaf(logged(scene, "child").then(scene.req("child")).focus_target());
        });
    });
    scene.forget();
    scene.request("parent");
    assert_eq!(scene.said(), ["parent Active"]);
    scene.forget();
    scene.request("child");
    assert_eq!(scene.said(), ["parent ActiveParent", "child Active"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusTest.kt, requestFocusWithDirection
#[test]
fn a_request_with_a_direction_gives_it_to_the_blocks_it_runs() {
    let heard = cell(None);
    let scene = scene(move |scene| {
        for (name, at) in [("first", 0.0), ("second", 500.0)] {
            let telling = properties(move |p| {
                p.set_on_enter(move |scope| heard.set(Some(scope.requested_focus_direction())))
            });
            Stack!(modifier = Modifier.offset(0.dp(), at.dp()).then(telling).focus_group(), {
                Leaf(scene.req(name).size(10.dp()).focus_target());
            });
        }
    });
    assert!(scene.requester("first").request_focus_with(Enter));
    scene.idle();
    assert_eq!(heard.get(), Some(Enter));
    assert!(scene.requester("second").request_focus_with(Exit));
    scene.idle();
    assert_eq!(heard.get(), Some(Exit));
}

// ---- Which target a requester stands for ------------------------------------------------------------

/// A request on what `content` has as `x`, with who ends focused among `a` and `b`, or a request
/// that nothing answers.
struct Stands {
    what: &'static str,
    content: fn(Scene),
    focused: &'static [&'static str],
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRequesterTest.kt, requestFocus_noFocusTargetInLayoutNode, requestFocus_focusTargetInLayoutNode_butBeforeFocusRequester, requestFocus_focusTargetInLayoutNode, requestFocus_focusTargetInChildLayoutNode, requestFocus_focusTargetAndReferenceInChildLayoutNode, requestFocus_focusTargetAndObserverInChildLayoutNode, requestFocus_focusTargetInDistantDescendantLayoutNode, requestFocus_firstFocusableChildIsFocused, requestFocus_firstFocusableChildIsFocused_differentDepths, requestFocus_onDeactivatedNode_focusesOnChild, requestFocus_intermediateDisabledParents_focusesOnDistantChild, requestFocus_DisabledParent_performsImplicitEnterIfCanFocusIsFalse, requestFocus_DisabledParent_implicitEnterCanBeCancelled, requestFocus_DisabledParents_selectsSiblingFromPathWhereCanFocusIsNotFalse, requestFocus_intermediateDisabledParents_focusesOnLeftMostChild, requestFocus_intermediateDisabledParents_focusesOnLeftMostChild_regardlessOfDepth, requestFocus_onDeactivatedNode_performsFocusEnter
#[test]
fn a_requester_stands_for_the_targets_after_it_and_below_it_that_can_take_the_focus() {
    let rows = [
        Stands {
            what: "no target",
            content: |scene| Leaf(scene.listen("a").then(scene.req("x"))),
            focused: &[],
        },
        Stands {
            what: "a target before the requester",
            content: |scene| Leaf(scene.listen("a").focus_target().then(scene.req("x"))),
            focused: &[],
        },
        Stands {
            what: "a target after it",
            content: |scene| Leaf(scene.listen("a").then(scene.req("x")).focus_target()),
            focused: &["a"],
        },
        Stands {
            what: "a target in a child",
            content: |scene| {
                Stack!(modifier = scene.req("x").then(scene.listen("a")), {
                    Leaf(Modifier.focus_target());
                });
            },
            focused: &["a"],
        },
        Stands {
            what: "the requester and a target in a child, heard above",
            content: |scene| {
                Stack!(modifier = scene.listen("a"), {
                    Leaf(scene.req("x").focus_target());
                });
            },
            focused: &["a"],
        },
        Stands {
            what: "the target and its listener in a child",
            content: |scene| {
                Stack!(modifier = scene.req("x"), {
                    Leaf(scene.listen("a").focus_target());
                });
            },
            focused: &["a"],
        },
        Stands {
            what: "a target far below",
            content: |scene| {
                Stack!(modifier = scene.listen("a").then(scene.req("x")), {
                    Stack!({
                        Stack!({
                            Stack!({
                                Leaf(Modifier.focus_target());
                            });
                        });
                    });
                });
            },
            focused: &["a"],
        },
        Stands {
            what: "the first of several",
            content: |scene| {
                Column!(modifier = scene.req("x"), {
                    Leaf(scene.listen("a").focus_target());
                    Leaf(scene.listen("b").focus_target());
                });
            },
            focused: &["a"],
        },
        Stands {
            what: "the first of several at different depths",
            content: |scene| {
                Column!(modifier = scene.req("x"), {
                    Stack!({
                        Leaf(scene.listen("a").focus_target());
                    });
                    Leaf(scene.listen("b").focus_target());
                });
            },
            focused: &["a"],
        },
        Stands {
            what: "the child of a target that can't take it",
            content: |scene| {
                Stack!(modifier = scene.req("x").then(cant_focus()).focus_target(), {
                    Leaf(Modifier.size(10.dp()).then(scene.listen("a")).focus_target());
                });
            },
            focused: &["a"],
        },
        Stands {
            what: "under several that can't take it",
            content: |scene| {
                Stack!(modifier = scene.req("x").then(cant_focus()).focus_target(), {
                    Stack!(modifier = cant_focus().focus_target(), {
                        Stack!(modifier = cant_focus().focus_target(), {
                            Leaf(scene.listen("a").focus_target());
                        });
                    });
                });
            },
            focused: &["a"],
        },
        Stands {
            what: "the enter of a target that can't take it cancelling",
            content: |scene| {
                let cancels = properties(|p| {
                    p.can_focus = false;
                    p.set_on_enter(|scope| scope.cancel_focus_change());
                });
                Stack!(modifier = scene.req("x").then(cancels).focus_target(), {
                    Leaf(Modifier.size(10.dp()).then(scene.listen("a")).focus_target());
                });
            },
            focused: &[],
        },
        Stands {
            what: "the enter of one below not cancelling by what it gives back",
            content: |scene| {
                Stack!(
                    modifier = scene.req("x").size(10.dp()).then(cant_focus()).focus_target(),
                    {
                        Leaf(Modifier.size(10.dp()).then(scene.listen("a")).focus_target());
                        let block = properties(|p| {
                            p.can_focus = false;
                            p.set_on_enter(|_| {});
                        });
                        Stack!(modifier = Modifier.size(10.dp()).then(block).focus_target(), {
                            Leaf(Modifier.size(10.dp()).focus_target());
                        });
                    }
                );
            },
            focused: &["a"],
        },
        Stands {
            what: "the leftmost, before a deeper one",
            content: |scene| {
                Row!(
                    modifier = Modifier
                        .size(100.dp())
                        .then(scene.req("x"))
                        .then(cant_focus())
                        .focus_target(),
                    {
                        Leaf(Modifier.size(10.dp()).then(scene.listen("a")).focus_target());
                        Stack!(
                            modifier = Modifier.size(10.dp()).then(cant_focus()).focus_target(),
                            {
                                Leaf(Modifier.size(10.dp()).focus_target());
                            }
                        );
                    }
                );
            },
            focused: &["a"],
        },
        Stands {
            what: "the leftmost, a deeper one itself",
            content: |scene| {
                Row!(
                    modifier = Modifier
                        .size(100.dp())
                        .then(scene.req("x"))
                        .then(cant_focus())
                        .focus_target(),
                    {
                        Stack!(
                            modifier = Modifier.size(10.dp()).then(cant_focus()).focus_target(),
                            {
                                Leaf(Modifier.size(10.dp()).then(scene.listen("a")).focus_target());
                            }
                        );
                        Leaf(Modifier.size(10.dp()).focus_target());
                    }
                );
            },
            focused: &["a"],
        },
        Stands {
            what: "where a deactivated target's enter sends it",
            content: |scene| {
                let there = scene.requester("second");
                let redirects = properties(move |p| {
                    p.set_on_enter(move |_| {
                        there.request_focus();
                    })
                });
                Column!(
                    modifier = scene.req("x").then(redirects).then(cant_focus()).focus_target(),
                    {
                        Leaf(Modifier.size(10.dp()).then(scene.listen("a")).focus_target());
                        Leaf(
                            Modifier
                                .size(10.dp())
                                .then(scene.listen("b"))
                                .then(scene.req("second"))
                                .focus_target(),
                        );
                    }
                );
            },
            focused: &["b"],
        },
    ];
    for row in rows {
        let scene = scene(row.content);
        scene.requester("x").request_focus();
        scene.idle();
        assert_eq!(scene.focused(), row.focused, "{}", row.what);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRequesterTest.kt, requestFocus_onDeactivatedParent_focusesOnChild
#[test]
fn a_request_on_a_deactivated_parent_goes_to_its_first_child_even_from_another() {
    let scene = scene(|scene| {
        Column!(modifier = scene.req("x").then(cant_focus()).focus_target(), {
            Leaf(Modifier.size(10.dp()).then(scene.listen("a")).focus_target());
            Leaf(Modifier.size(10.dp()).then(scene.target("b")));
        });
    });
    assert!(scene.request("b"));
    scene.request("x");
    assert_eq!(scene.focused(), ["a"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRequesterTest.kt, requestFocus_firstFocusableChildIsFocused_afterChange, requestFocus_onRemovedChild_skipsRemovedNode
#[test]
fn a_request_for_the_first_child_follows_children_coming_and_going() {
    for (shown_at_first, shown_then, expected) in [(false, true, "a"), (true, false, "b")] {
        let scene = scene(move |scene| {
            let shown = scene.flag("shown");
            Column!(modifier = scene.req("x"), {
                if shown.get() {
                    Leaf(scene.listen("a").focus_target());
                }
                Leaf(scene.listen("b").focus_target());
            });
        });
        scene.set("shown", shown_at_first);
        scene.set("shown", shown_then);
        scene.requester("x").request_focus();
        assert_eq!(scene.focused(), [expected]);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRequesterTest.kt, requestFocus_inOnDisposeOnRemovedChild_skipsRemovedNode
#[test]
fn a_request_from_the_disposal_of_a_child_skips_the_child_going() {
    let scene = scene(|scene| {
        let shown = scene.flag("shown");
        let requester = scene.requester("x");
        Column!(modifier = scene.req("x"), {
            if shown.get() {
                Leaf(scene.listen("a").focus_target());
                compose_runtime::DisposableEffect((), move |scope| {
                    scope.on_dispose(move || {
                        requester.request_focus();
                    })
                });
            }
            Leaf(scene.listen("b").focus_target());
        });
    });
    scene.set("shown", true);
    scene.set("shown", false);
    assert_eq!(scene.focused(), ["b"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRequesterTest.kt, requestFocusForAnyChild_triggersOnFocusChangedInParent
#[test]
fn a_listener_above_two_requesters_hears_either_after_the_focus_was_cleared() {
    let scene = scene(|scene| {
        Column!(modifier = scene.listen("parent"), {
            Leaf(scene.req("one").focus_target());
            Leaf(scene.req("two").focus_target());
        });
    });
    for name in ["one", "two"] {
        scene.clear(false);
        assert!(!scene.state("parent").is_focused());
        scene.request(name);
        assert!(scene.state("parent").is_focused());
    }
}

// ---- One requester, several targets, and none ---------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ReusedFocusRequesterTest.kt, oneComponent, twoComponents, focusRequesterUsedWithThreeComponent
#[test]
fn a_requester_on_several_targets_ends_on_the_last() {
    for names in [&["a"][..], &["a", "b"], &["a", "b", "c"]] {
        let names: &'static [&'static str] = names;
        let scene = scene(move |scene| {
            for &name in names {
                Leaf(scene.listen(name).then(scene.req("shared")).focus_target());
            }
        });
        assert!(scene.request("shared"));
        assert_eq!(scene.focused(), [*names.last().unwrap()]);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRequesterTest.kt, focusRequesterModifierNotUsed
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CaptureFocusTest.kt, focusRequesterModifierNotUsed
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FreeFocusTest.kt, focusRequesterModifierNotUsed
#[test]
fn a_requester_on_nothing_answers_no() {
    let scene = scene(|_| Stack!({}));
    assert!(!scene.request("x"));
    assert!(!scene.capture("x"));
    assert!(!scene.free("x"));
}

// ---- Capturing and freeing -----------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Act {
    Capture,
    Free,
}

/// `content`'s `first` focused, captured if asked, then the act on `x`.
struct Holding {
    what: &'static str,
    content: fn(Scene),
    first: Option<&'static str>,
    capture: Option<&'static str>,
    act: Act,
    answer: bool,
    states: &'static [(&'static str, FocusState)],
}

fn parent_with_focused_child(scene: Scene) {
    Stack!(modifier = scene.target("x"), {
        Leaf(scene.req("initial").focus_target());
    });
}

fn deactivated_focusable(scene: Scene) {
    Leaf(scene.named("x").then(cant_focus()).focusable());
}

fn deactivated_focusable_parent(scene: Scene) {
    Stack!(modifier = scene.named("x").then(cant_focus()).focusable(), {
        Leaf(scene.req("initial").focusable());
    });
}

fn shared_requester(scene: Scene) {
    Leaf(scene.listen("one").then(scene.req("x")).focus_target());
    Leaf(scene.listen("two").then(scene.req("initial")).then(scene.req("x")).focus_target());
}

fn shared_without_initial(scene: Scene) {
    Leaf(scene.listen("one").then(scene.req("x")).focus_target());
    Leaf(scene.listen("two").then(scene.req("x")).focus_target());
}

fn listening_target(scene: Scene) {
    Leaf(scene.listen("x").then(scene.req("x")).focus_target());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CaptureFocusTest.kt, active_captureFocus_changesStateToCaptured, activeParent_captureFocus_retainsStateAsActiveParent, captured_captureFocus_retainsStateAsCaptured, deactivated_captureFocus_retainsFocusState, deactivatedParent_captureFocus_retainsFocusState, inactive_captureFocus_retainsStateAsInactive
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FreeFocusTest.kt, active_freeFocus_retainFocusAsActive, activeParent_freeFocus_retainFocusAsActiveParent, captured_freeFocus_changesStateToActive, inactive_freeFocus_retainFocusAsInactive
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ReusedFocusRequesterCaptureFocusTest.kt, oneActiveComponent_returnsTrue, oneCapturedComponent_returnsTrue, oneInactiveComponent_returnsFalse, oneActiveOneInactiveComponent_returnsTrue, oneInactiveOneCapturedComponent_returnsTrue, twoInactiveComponent_returnsFalse
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ReusedFocusRequesterFreeFocusTest.kt, oneActiveComponent_returnsTrue, oneCapturedComponent_returnsTrue, oneInactiveComponent_returnsFalse, oneActiveOneInactiveComponent_returnsTrue, oneInactiveOneCapturedComponent_returnsTrue, twoInactiveComponent_returnsFalse
#[test]
fn capturing_and_freeing_answer_from_each_state_and_leave_it_as_it_should() {
    use Act::{Capture, Free};
    let row = |what, content, first, capture, act, answer, states| Holding {
        what,
        content,
        first,
        capture,
        act,
        answer,
        states,
    };
    let x = Some("x");
    let initial = Some("initial");
    for row in [
        row(
            "capture, active",
            listening_target as fn(Scene),
            x,
            None,
            Capture,
            true,
            &[("x", Captured)][..],
        ),
        row(
            "capture, a child focused",
            parent_with_focused_child,
            initial,
            None,
            Capture,
            false,
            &[("x", ActiveParent)],
        ),
        row("capture, captured", listening_target, x, x, Capture, true, &[("x", Captured)]),
        row(
            "capture, can't take it",
            deactivated_focusable,
            None,
            None,
            Capture,
            false,
            &[("x", Inactive)],
        ),
        row(
            "capture, can't take it, a child focused",
            deactivated_focusable_parent,
            initial,
            None,
            Capture,
            false,
            &[("x", ActiveParent)],
        ),
        row("capture, inactive", listening_target, None, None, Capture, false, &[("x", Inactive)]),
        row("free, active", listening_target, x, None, Free, true, &[("x", Active)]),
        row(
            "free, a child focused",
            parent_with_focused_child,
            initial,
            None,
            Free,
            false,
            &[("x", ActiveParent)],
        ),
        row("free, captured", listening_target, x, x, Free, true, &[("x", Active)]),
        row("free, inactive", listening_target, None, None, Free, false, &[("x", Inactive)]),
        row(
            "capture, shared, one active",
            shared_requester,
            initial,
            None,
            Capture,
            true,
            &[("one", Inactive), ("two", Captured)],
        ),
        row(
            "capture, shared, one captured",
            shared_requester,
            initial,
            initial,
            Capture,
            true,
            &[("one", Inactive), ("two", Captured)],
        ),
        row(
            "capture, shared, none",
            shared_without_initial,
            None,
            None,
            Capture,
            false,
            &[("one", Inactive), ("two", Inactive)],
        ),
        row(
            "free, shared, one active",
            shared_requester,
            initial,
            None,
            Free,
            true,
            &[("one", Inactive), ("two", Active)],
        ),
        row(
            "free, shared, one captured",
            shared_requester,
            initial,
            initial,
            Free,
            true,
            &[("one", Inactive), ("two", Active)],
        ),
        row(
            "free, shared, none",
            shared_without_initial,
            None,
            None,
            Free,
            false,
            &[("one", Inactive), ("two", Inactive)],
        ),
    ] {
        let scene = scene(row.content);
        if let Some(first) = row.first {
            scene.request(first);
        }
        if let Some(captured) = row.capture {
            assert!(scene.capture(captured), "{}", row.what);
        }
        let answer = match row.act {
            Capture => scene.capture("x"),
            Free => scene.free("x"),
        };
        assert_eq!(answer, row.answer, "{}", row.what);
        for &(name, state) in row.states {
            assert_eq!(scene.state(name), state, "{}: {name}", row.what);
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CaptureFocusTest.kt, captured_requestFocusForAnotherNode_retainsStateAsCaptured
#[test]
fn a_request_for_another_target_is_refused_while_the_focus_is_captured() {
    let scene = scene(|scene| {
        Leaf(scene.target("one"));
        Leaf(scene.target("two"));
    });
    assert!(scene.request("one"));
    assert!(scene.capture("one"));
    assert!(!scene.request("two"));
    assert_eq!(scene.states(["one", "two"]), [Captured, Inactive]);
}
