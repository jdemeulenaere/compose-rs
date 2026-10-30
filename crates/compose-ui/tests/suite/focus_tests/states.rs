//! What a listener hears: the state it's given as targets below it take the focus, and how often.

use super::support::*;

// ---- A parent's state, from what's below it ----------------------------------------------------------

/// (is focused, has focus, is captured)
type Heard = (bool, bool, bool);

const NOTHING: Heard = (false, false, false);
const FOCUSED: Heard = (true, true, false);
const BELOW: Heard = (false, true, false);
const CAPTURED: Heard = (true, true, true);

/// A focusable requested by the name `x`.
fn focusable_x(scene: Scene) {
    Leaf(scene.req("x").focusable());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusAggregationTest.kt, nonFocusableItem, focusableItem_notFocused, focusableItem_focused, parentOfNonFocusableItem, parentWithSingleFocusableChild_childNotFocused, parentWithSingleFocusableChild_childFocused, parentWithMultipleFocusableChildren_firstChildFocused, parentWithMultipleFocusableChildren_secondChildFocused, parentWithMultipleFocusableChildren_firstChildCapturesFocus, parentWithMultipleFocusableChildren_secondChildCapturesFocus, focusableParentWithMultipleFocusableChildren_childFocused, nonFocusableGrandParent_grandChildFocused, nonFocusableGrandParentWithMultipleGrandChildren_grandChildFocused, nonFocusableGrandParentWithMultipleChildrenAndGrandChildren_grandChildFocused
#[test]
fn a_listener_says_what_the_next_target_at_or_below_it_is() {
    // (what, content, requests x, captures x, what it hears)
    let rows: [(&str, fn(Scene), bool, bool, Heard); 14] = [
        ("no target", |scene| Leaf(scene.listen("l")), false, false, NOTHING),
        (
            "a focusable, unfocused",
            |scene| Leaf(scene.listen("l").then(scene.req("x")).focusable()),
            false,
            false,
            NOTHING,
        ),
        (
            "a focusable, focused",
            |scene| Leaf(scene.listen("l").then(scene.req("x")).focusable()),
            true,
            false,
            FOCUSED,
        ),
        (
            "over a child that isn't a target",
            |scene| {
                Stack!(modifier = scene.listen("l"), {
                    Stack!({});
                });
            },
            false,
            false,
            NOTHING,
        ),
        (
            "over a focusable, unfocused",
            |scene| {
                Stack!(modifier = scene.listen("l"), {
                    focusable_x(scene);
                });
            },
            false,
            false,
            NOTHING,
        ),
        (
            "over a focusable, focused: the same as the child's",
            |scene| {
                Stack!(modifier = scene.listen("l"), {
                    focusable_x(scene);
                });
            },
            true,
            false,
            FOCUSED,
        ),
        (
            "over two, the first focused",
            |scene| {
                Column!(modifier = scene.listen("l"), {
                    focusable_x(scene);
                    Leaf(Modifier.focusable());
                });
            },
            true,
            false,
            FOCUSED,
        ),
        (
            "over two, the second focused",
            |scene| {
                Column!(modifier = scene.listen("l"), {
                    Leaf(Modifier.focusable());
                    focusable_x(scene);
                });
            },
            true,
            false,
            FOCUSED,
        ),
        (
            "over two, the first captured",
            |scene| {
                Column!(modifier = scene.listen("l"), {
                    focusable_x(scene);
                    Leaf(Modifier.focusable());
                });
            },
            true,
            true,
            CAPTURED,
        ),
        (
            "over two, the second captured",
            |scene| {
                Column!(modifier = scene.listen("l"), {
                    Leaf(Modifier.focusable());
                    focusable_x(scene);
                });
            },
            true,
            true,
            CAPTURED,
        ),
        (
            "on a focusable with a child focused",
            |scene| {
                Column!(modifier = scene.listen("l").focusable(), {
                    Leaf(Modifier.focusable());
                    focusable_x(scene);
                });
            },
            true,
            false,
            BELOW,
        ),
        (
            "over a focusable with its child focused",
            |scene| {
                Column!(modifier = scene.listen("l"), {
                    Row!(modifier = Modifier.focusable(), {
                        focusable_x(scene);
                    });
                });
            },
            true,
            false,
            BELOW,
        ),
        (
            "over a focusable with its second child focused",
            |scene| {
                Column!(modifier = scene.listen("l"), {
                    Row!(modifier = Modifier.focusable(), {
                        Leaf(Modifier.focusable());
                        focusable_x(scene);
                    });
                });
            },
            true,
            false,
            BELOW,
        ),
        (
            "over two of them, the second's second child focused",
            |scene| {
                Column!(modifier = scene.listen("l"), {
                    Row!(modifier = Modifier.focusable(), {
                        Leaf(Modifier.focusable());
                        Leaf(Modifier.focusable());
                    });
                    Row!(modifier = Modifier.focusable(), {
                        Leaf(Modifier.focusable());
                        focusable_x(scene);
                    });
                });
            },
            true,
            false,
            BELOW,
        ),
    ];
    for (what, content, requests, captures, expected) in rows {
        let scene = scene(content);
        if requests {
            assert!(scene.request("x"), "{what}");
        }
        if captures {
            assert!(scene.capture("x"), "{what}");
        }
        let state = scene.state("l");
        assert_eq!(
            (state.is_focused(), state.has_focus(), state.is_captured()),
            expected,
            "{what}"
        );
    }
}

// ---- How often ---------------------------------------------------------------------------------------

/// What's heard, and how: the three ways to listen.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Way {
    /// `on_focus_event`, its element staying equal across compositions.
    Events,
    /// `on_focus_event` with a new element in each composition: a node updated, as the arm of
    /// Compose's test made with an object for a modifier.
    EventsAnew,
    /// `on_focus_changed`: an event only if it's a change.
    Changes,
}

const WAYS: [Way; 3] = [Way::Events, Way::EventsAnew, Way::Changes];

fn listener(scene: Scene, way: Way) -> Modifier {
    match way {
        Way::Events => scene.events("heard"),
        Way::EventsAnew => scene.events_anew("heard"),
        Way::Changes => scene.listen("heard"),
    }
}

fn heard(scene: Scene, way: Way) -> Vec<FocusState> {
    match way {
        Way::Events | Way::EventsAnew => scene.event_states("heard"),
        Way::Changes => scene.heard("heard"),
    }
}

/// A row of how often: what's composed, what's done first, what's done then, and what each way
/// hears from what's done then, in the order of `WAYS`.
struct Count {
    what: &'static str,
    content: fn(Scene, Way),
    first: fn(Scene),
    then: fn(Scene),
    heard: [&'static [FocusState]; 3],
}

fn nothing(_: Scene) {}

fn asks(scene: Scene) {
    scene.request("x");
}

/// `listener` on a target asked for by `x`.
fn on_a_target(scene: Scene, way: Way) {
    Leaf(listener(scene, way).then(scene.req("x")).focus_target());
}

fn check_counts(rows: &[Count]) {
    for row in rows {
        for (index, way) in WAYS.into_iter().enumerate() {
            let content = row.content;
            let scene = scene(move |scene| content(scene, way));
            (row.first)(*scene);
            scene.forget();
            (row.then)(*scene);
            assert_eq!(heard(*scene, way), row.heard[index], "{}, {way:?}", row.what);
        }
    }
}

const NONE: &[FocusState] = &[];

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusEventCountTest.kt, whenFocusIsGained_onFocusEventIsCalledOnce, whenFocusMovesWithinParent_onFocusEventIsNotCalled, whenFocusIsGained_onFocusEventIsCalledOnParent, whenFocusIsGained_onFocusEventIsCalledOnLocalParent, requestingFocusWhenAlreadyFocused_onFocusEventIsCalledAgain, whenFocusIsLost_onFocusEventIsCalledOnce
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedCountTest.kt, whenFocusIsGained_focusChangedIsCalledOnce, requestingFocusWhenAlreadyFocused_onFocusChangedIsNotCalledAgain, whenFocusIsLost_focusChangedIsCalledOnce
#[test]
fn a_listener_hears_each_change_once_and_an_event_of_a_request_that_changes_nothing() {
    check_counts(&[
        Count {
            what: "gained",
            content: on_a_target,
            first: nothing,
            then: asks,
            heard: [&[Active], &[Active], &[Active]],
        },
        Count {
            what: "moving within the parent",
            content: |scene, way| {
                Stack!(modifier = listener(scene, way).focus_target(), {
                    Leaf(scene.req("one").focus_target());
                    Leaf(scene.req("two").focus_target());
                });
            },
            first: |scene| {
                scene.request("one");
            },
            then: |scene| {
                scene.request("two");
            },
            heard: [NONE, NONE, NONE],
        },
        Count {
            what: "gained by a child",
            content: |scene, way| {
                Stack!(modifier = listener(scene, way).focus_target(), {
                    Leaf(scene.req("x").focus_target());
                });
            },
            first: nothing,
            then: asks,
            heard: [&[ActiveParent], &[ActiveParent], &[ActiveParent]],
        },
        Count {
            what: "gained by a target further along the chain",
            content: |scene, way| {
                Leaf(listener(scene, way).focus_target().then(scene.req("x")).focus_target());
            },
            first: nothing,
            then: asks,
            heard: [&[ActiveParent], &[ActiveParent], &[ActiveParent]],
        },
        Count {
            what: "asking again",
            content: on_a_target,
            first: asks,
            then: asks,
            heard: [&[Active], &[Active], NONE],
        },
        Count {
            what: "lost",
            content: on_a_target,
            first: asks,
            then: |scene| scene.clear(false),
            heard: [&[Inactive], &[Inactive], &[Inactive]],
        },
    ]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusEventCountTest.kt, initially_onFocusEventIsCalledOnce, initiallyNoFocusTarget_onFocusEventIsCalledOnce
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedCountTest.kt, initially_focusChangedIsCalledOnce, initiallyNoFocusTarget_onFocusChangedIsCalledOnce
#[test]
fn a_listener_is_told_once_at_first_with_a_target_or_without() {
    let contents: [fn(Scene, Way); 2] = [on_a_target, |scene, way| Leaf(listener(scene, way))];
    for content in contents {
        for way in WAYS {
            let scene = scene(move |scene| content(scene, way));
            assert_eq!(heard(*scene, way), [Inactive], "{way:?}");
        }
    }
}

/// `listener`, a target there or not by the flag `shown`, and the requester `x`.
fn target_shown_on_chain(scene: Scene, way: Way) {
    let shown = scene.flag("shown").get();
    Leaf(listener(scene, way).then(scene.req("x")).then(if shown {
        Modifier.focus_target()
    } else {
        Modifier
    }));
}

/// The same, with another target after it that the requester goes to.
fn target_shown_above_another(scene: Scene, way: Way) {
    let shown = scene.flag("shown").get();
    Leaf(
        listener(scene, way)
            .then(if shown { Modifier.focus_target() } else { Modifier })
            .then(scene.req("x"))
            .focus_target(),
    );
}

/// `properties` there or not by the flag `shown`.
fn properties_shown(scene: Scene, way: Way, block: fn(&mut FocusProperties)) {
    let shown = scene.flag("shown").get();
    Leaf(
        listener(scene, way).then(if shown { properties(block) } else { Modifier }).focus_target(),
    );
}

fn show(scene: Scene) {
    scene.set("shown", true);
}

fn hide(scene: Scene) {
    scene.set("shown", false);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusEventCountTest.kt, removingActiveComposable_onFocusEventIsCalledWithDefaultValue, removingActiveFocusNode_onFocusEventIsCalledTwice, removingInactiveFocusNode_onFocusEventIsCalledOnce, removingInactiveFocusNode_withActiveChild_onFocusEventIsCalledOnce, removingInactiveFocusNode_withActiveChildLayout_onFocusEventIsCalledOnce, addingFocusTarget_onFocusEventIsCalledOnce, addingFocusEventAndTargetToActiveChild_onFocusEventIsCalledOnce
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedCountTest.kt, removingActiveFocusNode_onFocusChangedIsCalledOnce, removingInactiveFocusNode_onFocusChangedIsNotCalled, addingFocusTarget_onFocusChangedIsNotCalled
#[test]
fn a_target_coming_or_going_tells_only_what_changed_and_a_new_element_hears_its_state_again() {
    check_counts(&[
        Count {
            what: "the composable with the focus removed",
            content: |scene, way| {
                if !scene.flag("removed").get() {
                    on_a_target(scene, way);
                }
            },
            first: asks,
            then: |scene| scene.set("removed", true),
            heard: [&[Inactive], &[Inactive], &[Inactive]],
        },
        Count {
            what: "the target with the focus removed",
            content: target_shown_on_chain,
            first: |scene| {
                show(scene);
                asks(scene);
            },
            then: hide,
            heard: [&[Inactive], &[Inactive, Inactive], &[Inactive]],
        },
        Count {
            what: "an inactive target removed",
            content: target_shown_on_chain,
            first: show,
            then: hide,
            heard: [NONE, &[Inactive], NONE],
        },
        Count {
            what: "an inactive target removed from above another with the focus",
            content: target_shown_above_another,
            first: |scene| {
                show(scene);
                asks(scene);
            },
            then: hide,
            heard: [NONE, &[Active], NONE],
        },
        Count {
            what: "a target removed from over a child with the focus",
            content: |scene, way| {
                let shown = scene.flag("shown").get();
                Stack!(modifier = listener(scene, way), {
                    Stack!(modifier = if shown { Modifier.focus_target() } else { Modifier }, {
                        Leaf(scene.req("x").focus_target());
                    });
                });
            },
            first: |scene| {
                show(scene);
                asks(scene);
            },
            then: hide,
            heard: [NONE, &[Active], NONE],
        },
        Count {
            what: "a target added",
            content: target_shown_on_chain,
            first: nothing,
            then: show,
            heard: [NONE, &[Inactive], NONE],
        },
        Count {
            what: "a listener and a target added over a child with the focus",
            content: |scene, way| {
                let shown = scene.flag("shown").get();
                let added = if shown { listener(scene, way).focus_target() } else { Modifier };
                Stack!(modifier = added, {
                    Leaf(scene.req("x").focus_target());
                });
            },
            first: asks,
            then: show,
            heard: [&[ActiveParent], &[ActiveParent], &[ActiveParent]],
        },
    ]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusEventCountTest.kt, addingEmptyFocusProperties_onFocusEventIsTriggered, addingCanFocusProperty_onFocusEventIsTriggered, addingCantFocusProperty_noFocusEventIsTriggered, removingCanFocusProperty_onFocusEventIsTriggered, removingCantFocusProperty_onFocusEventIsTriggered
#[test]
fn focus_properties_coming_or_going_tell_only_a_new_element() {
    const ONLY_ANEW: [&[FocusState]; 3] = [NONE, &[Inactive], NONE];
    check_counts(&[
        Count {
            what: "empty added",
            content: |scene, way| properties_shown(scene, way, |_| {}),
            first: nothing,
            then: show,
            heard: ONLY_ANEW,
        },
        Count {
            what: "can_focus true added",
            content: |scene, way| properties_shown(scene, way, |p| p.can_focus = true),
            first: nothing,
            then: show,
            heard: ONLY_ANEW,
        },
        Count {
            what: "can_focus false added",
            content: |scene, way| properties_shown(scene, way, |p| p.can_focus = false),
            first: nothing,
            then: show,
            heard: ONLY_ANEW,
        },
        Count {
            what: "can_focus true removed",
            content: |scene, way| properties_shown(scene, way, |p| p.can_focus = true),
            first: show,
            then: hide,
            heard: ONLY_ANEW,
        },
        Count {
            what: "can_focus false removed",
            content: |scene, way| properties_shown(scene, way, |p| p.can_focus = false),
            first: show,
            then: hide,
            heard: ONLY_ANEW,
        },
    ]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusEventCountTest.kt, deactivatingFocusNode_noFocusEventIsCalledOnce, activatingFocusNode_doesNotTriggerFocusEvent, changingFocusProperty_onFocusEventIsNotCalled
#[test]
fn a_change_read_by_a_property_block_tells_no_listener() {
    let can_focus: fn(Scene, Way) = |scene, way| {
        let flag = scene.flag("shown");
        Leaf(
            listener(scene, way).then(properties(move |p| p.can_focus = flag.get())).focus_target(),
        );
    };
    check_counts(&[
        Count {
            what: "deactivated",
            content: can_focus,
            first: show,
            then: hide,
            heard: [NONE, NONE, NONE],
        },
        Count {
            what: "activated",
            content: can_focus,
            first: nothing,
            then: show,
            heard: [NONE, NONE, NONE],
        },
        Count {
            what: "next changed",
            content: |scene, way| {
                let flag = scene.flag("shown");
                let (one, two) = (scene.requester("one"), scene.requester("two"));
                Leaf(
                    listener(scene, way)
                        .then(properties(move |p| p.next = if flag.get() { two } else { one }))
                        .focus_target(),
                );
            },
            first: nothing,
            then: show,
            heard: [NONE, NONE, NONE],
        },
    ]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusEventCountTest.kt, addingFocusEventsToDelegatingNodes_focusedChild_sendsCorrectFocusEvents, addingFocusEventsToDelegatingNodes_focusedParent_sendsCorrectFocusEvents
#[test]
fn a_listener_added_to_a_node_holding_a_target_hears_its_state() {
    for (asked_on_child, parent_state, child_state) in
        [(true, ActiveParent, Active), (false, Active, Inactive)]
    {
        let (parent, child) =
            (HeldTarget::new(Focusability::ALWAYS), HeldTarget::new(Focusability::ALWAYS));
        let scene = scene(move |scene| {
            let shown = scene.flag("shown").get();
            let parent_events = if shown { scene.events("parent") } else { Modifier };
            let child_events = if shown { scene.events("child") } else { Modifier };
            Stack!(modifier = parent_events.then(parent.modifier()), {
                Leaf(child_events.clone().then(child.modifier()));
            });
        });
        assert!(if asked_on_child { child.request() } else { parent.request() });
        scene.set("shown", true);
        assert_eq!(parent.state(), parent_state);
        assert_eq!(child.state(), child_state);
        assert_eq!(scene.event_states("parent"), [parent_state]);
        assert_eq!(scene.event_states("child"), [child_state]);
    }
}
