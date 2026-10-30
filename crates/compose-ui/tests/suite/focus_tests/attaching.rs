//! Targets coming and going: the states left for what's around them, the focus they take with
//! them, and nodes reused.
//!
//! The listeners here are new at each composition: an element updated hears the state of the
//! target it now covers, which is what these rows are about.

use compose_runtime::{ReusableContent, ReusableContentHost};

use super::support::*;

/// `modifier` while `on`, else nothing.
fn when(on: bool, modifier: Modifier) -> Modifier {
    if on { modifier } else { Modifier }
}

/// A row: `content` with the flag `on` true at first or not, what's done first, the state heard
/// by `x` after that if it's asked, then the state of `x` when the flag is flipped.
struct Flip {
    what: &'static str,
    content: fn(Scene, bool),
    on_at_first: bool,
    capture: bool,
    before: FocusState,
    after: FocusState,
}

fn check_flips(rows: &[Flip]) {
    for row in rows {
        let content = row.content;
        let scene = Scene::empty();
        scene.flag("on").set(row.on_at_first);
        let scene = scene.compose(move |scene| content(scene, scene.flag("on").get()));
        scene.request("y");
        if row.capture {
            assert!(scene.capture("y"), "{}", row.what);
        }
        assert_eq!(scene.state("x"), row.before, "{}: before", row.what);
        scene.set("on", !row.on_at_first);
        assert_eq!(scene.state("x"), row.after, "{}: after", row.what);
    }
}

fn hearing(scene: Scene) -> Modifier {
    scene.listen_anew("x")
}

fn optional_target_over_a_child(scene: Scene, on: bool) {
    Stack!(
        modifier = hearing(scene).then(scene.req("y")).then(when(on, Modifier.focus_target())),
        {
            Leaf(Modifier.focus_target());
        }
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, reorderedFocusRequesterModifiers_onFocusChangedInSameModifierChain, removedModifier_onFocusChangedDoesNotHaveAFocusTarget, removedFocusTarget_withNoNextFocusTarget, removedActiveFocusTarget_pointsToNextFocusTarget, removedActiveFocusTargetAndFocusChanged_triggersOnFocusEvent, removedActiveComposable_doesNotTriggerOnFocusEvent, removedCapturedFocusTarget_pointsToNextFocusTarget
#[test]
fn removing_the_target_with_the_focus_clears_it_and_a_listener_hears_what_it_covers_now() {
    check_flips(&[
        Flip {
            what: "the chain reordered, the listener now past the target with the focus",
            content: |scene, on| {
                let first = hearing(scene).then(scene.req("y"));
                let chain = if on {
                    first.focus_target().focus_target()
                } else {
                    Modifier.focus_target().then(first).focus_target()
                };
                Leaf(chain);
            },
            on_at_first: true,
            capture: false,
            before: Active,
            after: Inactive,
        },
        Flip {
            what: "the chain reordered, the listener now without a target",
            content: |scene, on| {
                let first = hearing(scene).then(scene.req("y"));
                Leaf(if on { first.focus_target() } else { Modifier.focus_target().then(first) });
            },
            on_at_first: true,
            capture: false,
            before: Active,
            after: Inactive,
        },
        Flip {
            what: "no target after it",
            content: |scene, on| {
                Leaf(hearing(scene).then(scene.req("y")).then(when(on, Modifier.focus_target())));
            },
            on_at_first: true,
            capture: false,
            before: Active,
            after: Inactive,
        },
        Flip {
            what: "a child's target after it, which didn't have it",
            content: optional_target_over_a_child,
            on_at_first: true,
            capture: false,
            before: Active,
            after: Inactive,
        },
        Flip {
            what: "captured",
            content: optional_target_over_a_child,
            on_at_first: true,
            capture: true,
            before: Captured,
            after: Inactive,
        },
        Flip {
            what: "the listener and the target going together",
            content: |scene, on| {
                Leaf(scene.req("y").then(when(on, scene.events_anew("heard").focus_target())));
            },
            on_at_first: true,
            capture: false,
            before: Inactive,
            after: Inactive,
        },
    ]);

    // Those last: heard by `on_focus_event`.
    let scene = scene(|scene| {
        let shown = !scene.flag("gone").get();
        Leaf(scene.req("y").then(when(shown, scene.events_anew("x").focus_target())));
    });
    assert!(scene.request("y"));
    assert_eq!(scene.state("x"), Active);
    scene.set("gone", true);
    assert_eq!(scene.state("x"), Inactive);

    let scene = self::scene(|scene| {
        if !scene.flag("gone").get() {
            Leaf(scene.req("y").then(scene.events_anew("x")).focus_target());
        }
    });
    assert!(scene.request("y"));
    assert_eq!(scene.state("x"), Active);
    scene.set("gone", true);
    assert_eq!(scene.state("x"), Inactive);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, removeActiveFocusTarget_updatesFocusOwner, removedActiveComposable_updatesFocusOwner
#[test]
fn removing_the_target_with_the_focus_leaves_the_owner_with_nothing_active() {
    let contents: [fn(Scene); 2] = [
        |scene| {
            let shown = !scene.flag("gone").get();
            Leaf(scene.req("y").then(when(shown, Modifier.focus_target())));
        },
        |scene| {
            Stack!({
                if !scene.flag("gone").get() {
                    Leaf(scene.req("y").focus_target());
                }
            });
        },
    ];
    for content in contents {
        let scene = scene(content);
        assert!(scene.request("y"));
        assert!(scene.root_has_focus());
        scene.set("gone", true);
        assert!(!scene.root_has_focus());
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, removedActiveParentFocusTarget_pointsToNextFocusTarget, removedActiveParentFocusTarget_withNoNextFocusTarget, removedDeactivatedParentFocusTarget_pointsToNextFocusTarget, removedDeactivatedParentFocusTarget_pointsToNextDeactivatedParentFocusTarget, removedInactiveFocusTarget_pointsToNextFocusTarget
#[test]
fn a_listener_goes_on_to_the_next_target_when_the_one_before_it_is_removed() {
    check_flips(&[
        Flip {
            what: "a parent of what has the focus removed",
            content: |scene, on| {
                Stack!(modifier = hearing(scene).then(when(on, Modifier.focus_target())), {
                    Leaf(scene.req("y").focus_target());
                });
            },
            on_at_first: true,
            capture: false,
            before: ActiveParent,
            after: Active,
        },
        Flip {
            what: "the target and its parent in one chain removed",
            content: |scene, on| {
                Leaf(
                    hearing(scene).then(when(
                        on,
                        Modifier.focus_target().then(scene.req("y")).focus_target(),
                    )),
                );
            },
            on_at_first: true,
            capture: false,
            before: ActiveParent,
            after: Inactive,
        },
        Flip {
            what: "a deactivated parent of what has the focus removed",
            content: |scene, on| {
                Stack!(modifier = hearing(scene).then(when(on, cant_focus().focus_target())), {
                    Leaf(scene.req("y").focus_target());
                });
            },
            on_at_first: true,
            capture: false,
            before: ActiveParent,
            after: Active,
        },
        Flip {
            what: "a deactivated parent over another removed",
            content: |scene, on| {
                Stack!(modifier = hearing(scene).then(when(on, cant_focus().focus_target())), {
                    Stack!(
                        modifier = scene.listen_anew("inner").then(cant_focus()).focus_target(),
                        {
                            Leaf(scene.req("y").focus_target());
                        }
                    );
                });
            },
            on_at_first: true,
            capture: false,
            before: ActiveParent,
            after: ActiveParent,
        },
        Flip {
            what: "an inactive target removed from before the one with the focus",
            content: |scene, on| {
                Leaf(
                    hearing(scene)
                        .then(when(on, Modifier.focus_target()))
                        .then(scene.req("y"))
                        .focus_target(),
                );
            },
            on_at_first: true,
            capture: false,
            before: ActiveParent,
            after: Active,
        },
    ]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, removedDeactivatedParent_parentsFocusTarget_isUnchanged, removedDeactivatedParentAndActiveChild_grandparent_retainsDeactivatedState, removedDeactivatedParentAndActiveChild_grandParent_retainsNonDeactivatedState, removedActiveParentFocusTargetAndFocusedChild_clearsFocusFromAllParents, removedActiveComposable_clearsFocusFromAllParents
#[test]
fn a_grandparent_keeps_having_the_focus_only_if_something_below_it_still_does() {
    check_flips(&[
        Flip {
            what: "a deactivated parent removed, its child keeping the focus",
            content: |scene, on| {
                Stack!(modifier = hearing(scene).then(cant_focus()).focus_target(), {
                    Stack!(modifier = when(on, cant_focus().focus_target()), {
                        Leaf(scene.req("y").focus_target());
                    });
                });
            },
            on_at_first: true,
            capture: false,
            before: ActiveParent,
            after: ActiveParent,
        },
        Flip {
            what: "a deactivated parent and the child with the focus removed, under a deactivated one",
            content: |scene, on| {
                Stack!(modifier = hearing(scene).then(cant_focus()).focus_target(), {
                    Stack!(modifier = when(on, cant_focus().focus_target()), {
                        Leaf(scene.req("y").then(when(on, Modifier.focus_target())));
                    });
                });
            },
            on_at_first: true,
            capture: false,
            before: ActiveParent,
            after: Inactive,
        },
        Flip {
            what: "a deactivated parent and the child with the focus removed, under a target",
            content: |scene, on| {
                Stack!(modifier = hearing(scene).focus_target(), {
                    Stack!(modifier = when(on, cant_focus().focus_target()), {
                        Leaf(scene.req("y").then(when(on, Modifier.focus_target())));
                    });
                });
            },
            on_at_first: true,
            capture: false,
            before: ActiveParent,
            after: Inactive,
        },
        Flip {
            what: "two in a chain removed, under a target",
            content: |scene, on| {
                Stack!(modifier = hearing(scene).focus_target(), {
                    Leaf(when(on, Modifier.focus_target().then(scene.req("y")).focus_target()));
                });
            },
            on_at_first: true,
            capture: false,
            before: ActiveParent,
            after: Inactive,
        },
        Flip {
            what: "the composable with the focus removed, under a target",
            content: |scene, on| {
                Stack!(modifier = hearing(scene).focus_target(), {
                    if on {
                        Leaf(scene.req("y").focus_target());
                    }
                });
            },
            on_at_first: true,
            capture: false,
            before: ActiveParent,
            after: Inactive,
        },
    ]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, addedFocusTarget_pointsToTheFocusTargetJustAdded, addedFocusTarget_withNoNextFocusTarget, addedMultipleFocusTargets_withinActiveNode, addedMultipleFocusTargets_withinActiveHierarchy, addedFocusTarget_withinCapturedNode, addedFocusTarget_withinCapturedHierarchy
#[test]
fn a_target_added_above_the_focus_has_it_below_it_and_one_added_below_has_nothing() {
    check_flips(&[
        Flip {
            what: "added between a listener and a child with the focus",
            content: |scene, on| {
                Stack!(
                    modifier =
                        hearing(scene).then(scene.req("y")).then(when(on, Modifier.focus_target())),
                    {
                        Leaf(Modifier.focus_target());
                    }
                );
            },
            on_at_first: false,
            capture: false,
            before: Active,
            after: ActiveParent,
        },
        Flip {
            what: "added where the requester had none",
            content: |scene, on| {
                Leaf(hearing(scene).then(scene.req("y")).then(when(on, Modifier.focus_target())));
            },
            on_at_first: false,
            capture: false,
            before: Inactive,
            after: Inactive,
        },
        Flip {
            what: "added before a target with the focus, in its chain, captured",
            content: |scene, on| {
                Leaf(when(on, hearing(scene).focus_target()).then(scene.req("y")).focus_target());
            },
            on_at_first: false,
            capture: true,
            before: Inactive,
            after: ActiveParent,
        },
        Flip {
            what: "added over a child with the focus, captured",
            content: |scene, on| {
                Stack!(modifier = when(on, hearing(scene).focus_target()), {
                    Leaf(scene.req("y").focus_target());
                });
            },
            on_at_first: false,
            capture: true,
            before: Inactive,
            after: ActiveParent,
        },
    ]);

    // Several at once, in one chain and in layouts above.
    let contents: [fn(Scene, bool); 2] = [
        |scene, on| {
            Leaf(
                when(on, scene.listen_anew("one").focus_target())
                    .then(when(on, scene.listen_anew("two").focus_target()))
                    .then(scene.req("y"))
                    .focus_target(),
            );
        },
        |scene, on| {
            Stack!(modifier = when(on, scene.listen_anew("one").focus_target()), {
                Stack!(modifier = when(on, scene.listen_anew("two").focus_target()), {
                    Leaf(scene.req("y").focus_target());
                });
            });
        },
    ];
    for content in contents {
        let scene = scene(move |scene| content(scene, scene.flag("on").get()));
        assert!(scene.request("y"));
        scene.set("on", true);
        assert_eq!(scene.states(["one", "two"]), [ActiveParent; 2]);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, addedFocusTarget_withinInactiveNode, addedMultipleFocusTargets_withinInactiveHierarchy, addedFocusTarget_withinActiveNode, addedFocusTarget_withinActiveHierarchy
#[test]
fn a_node_added_says_from_its_own_state_what_is_below_it() {
    // Into a chain and into a layout with nothing focused.
    let held = [HeldTarget::new(Focusability::ALWAYS), HeldTarget::new(Focusability::ALWAYS)];
    let [first, second] = held;
    let scene = scene(move |scene| {
        let on = scene.flag("on").get();
        Leaf(first.modifier().then(when(on, second.modifier())));
    });
    scene.set("on", true);
    assert_eq!((first.state(), second.state()), (Inactive, Inactive));

    let held = [
        HeldTarget::new(Focusability::ALWAYS),
        HeldTarget::new(Focusability::ALWAYS),
        HeldTarget::new(Focusability::ALWAYS),
    ];
    let [parent, one, two] = held;
    let scene = self::scene(move |scene| {
        let on = scene.flag("on").get();
        Column!(modifier = parent.modifier(), {
            if on {
                Leaf(one.modifier());
                Leaf(two.modifier());
            }
        });
    });
    scene.set("on", true);
    assert_eq!((parent.state(), one.state(), two.state()), (Inactive, Inactive, Inactive));

    // Over what has the focus, in its chain and in a layout above it.
    let added = HeldTarget::new(Focusability::ALWAYS);
    let scene = self::scene(move |scene| {
        let on = scene.flag("on").get();
        Leaf(when(on, hearing(scene).then(added.modifier())).then(scene.req("y")).focus_target());
    });
    assert!(scene.request("y"));
    scene.set("on", true);
    assert_eq!(added.state(), ActiveParent);
    assert_eq!(scene.state("x"), ActiveParent);

    let added = HeldTarget::new(Focusability::ALWAYS);
    let scene = self::scene(move |scene| {
        let on = scene.flag("on").get();
        Column!(modifier = when(on, hearing(scene).then(added.modifier())), {
            Leaf(Modifier.focus_target());
            Leaf(scene.req("y").focus_target());
        });
    });
    assert!(scene.request("y"));
    scene.set("on", true);
    assert_eq!(added.state(), ActiveParent);
    assert_eq!(scene.state("x"), ActiveParent);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, removingDeactivatedItem_withNoNextFocusTarget, removingDeactivatedItem_withDeactivatedNextFocusTarget
#[test]
fn removing_a_deactivated_target_tells_no_one_it_had_the_focus() {
    let contents: [fn(Scene, bool); 2] = [
        |scene, on| {
            Leaf(hearing(scene).then(when(on, cant_focus().focus_target())));
        },
        |scene, on| {
            Stack!(modifier = hearing(scene).then(when(on, cant_focus().focus_target())), {
                Leaf(cant_focus().focus_target());
            });
        },
    ];
    for content in contents {
        let scene = Scene::empty();
        scene.flag("on").set(true);
        let scene = scene.compose(move |scene| content(scene, scene.flag("on").get()));
        scene.set("on", false);
        assert!(!scene.state("x").is_focused());
    }
}

// ---- Nodes reused -----------------------------------------------------------------------------------

/// The ways to have the held target when its content is reused: with the focus or with a child's.
#[derive(Clone, Copy, PartialEq)]
enum Holds {
    Nothing,
    Focus,
    Capture,
    Child,
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, reuseInactiveFocusTarget_stateInactive, reuseActiveFocusTarget_stateInactive, reuseCapturedFocusTarget_stateInactive, reuseActiveParentFocusTarget_stateInactive
#[test]
fn a_target_reused_starts_again_without_the_focus() {
    for (holds, before) in [
        (Holds::Nothing, Inactive),
        (Holds::Focus, Active),
        (Holds::Capture, Captured),
        (Holds::Child, ActiveParent),
    ] {
        let held = HeldTarget::new(Focusability::ALWAYS);
        let key = shared(0);
        let scene = scene(move |scene| {
            ReusableContent(key.get(), move || {
                if holds == Holds::Child {
                    Stack!(modifier = held.modifier(), {
                        Leaf(scene.req("y").focus_target());
                    });
                } else {
                    Leaf(scene.req("y").then(held.modifier()));
                }
            });
        });
        if holds != Holds::Nothing {
            assert!(scene.request("y"));
        }
        if holds == Holds::Capture {
            assert!(scene.capture("y"));
        }
        assert_eq!(held.state(), before);
        key.set(1);
        scene.idle();
        assert_eq!(held.state(), Inactive);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, reusedContent_clearsFocusOnReset
#[test]
fn a_target_in_content_put_aside_for_reuse_loses_the_focus() {
    let held = HeldTarget::new(Focusability::ALWAYS);
    let active = shared(true);
    let scene = scene(move |scene| {
        ReusableContentHost(active.get(), move || {
            Leaf(scene.req("y").size(50.dp()).then(held.modifier()));
        });
    });
    assert!(scene.request("y"));
    assert_eq!(held.state(), Active);
    active.set(false);
    scene.idle();
    assert_eq!(held.state(), Inactive);
}

// ---- A node that is a target and something else --------------------------------------------------------

/// A node holding a target as a delegate that hears keys, itself or through another delegate,
/// and that hears focus events, itself or through another.
struct Combined {
    through_delegate: bool,
    scene: Scene,
}

struct CombinedElement(Combined);

impl ModifierNodeElement for CombinedElement {
    type Node = CombinedNode;
    fn create(&self) -> CombinedNode {
        let mut delegates = Delegates::new();
        delegates.delegate(focus_target_modifier_node(Focusability::ALWAYS, None));
        if self.0.through_delegate {
            delegates.delegate(Hearing { scene: self.0.scene });
        }
        CombinedNode {
            delegates,
            hearing: (!self.0.through_delegate).then_some(Hearing { scene: self.0.scene }),
        }
    }
    fn update(&self, _node: &mut CombinedNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "combined"
    }
}

struct CombinedNode {
    delegates: Delegates,
    hearing: Option<Hearing>,
}

impl ModifierNode for CombinedNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        self.hearing.as_mut().map(|hearing| hearing as &mut dyn KeyInputModifierNode)
    }
    fn as_focus_event(&mut self) -> Option<&mut dyn FocusEventModifierNode> {
        self.hearing.as_mut().map(|hearing| hearing as &mut dyn FocusEventModifierNode)
    }
}

struct Hearing {
    scene: Scene,
}

impl ModifierNode for Hearing {
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
    fn as_focus_event(&mut self) -> Option<&mut dyn FocusEventModifierNode> {
        Some(self)
    }
}

impl KeyInputModifierNode for Hearing {
    fn on_key_event(&mut self, _event: &KeyEvent, _context: &mut EventContext<'_>) -> bool {
        self.scene.say("key");
        true
    }
}

impl FocusEventModifierNode for Hearing {
    fn on_focus_event(&mut self, state: FocusState, _context: &mut EventContext<'_>) {
        self.scene.say(format!("{state:?}"));
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, focusTarget_nodeThatIsKeyInputNodeKind_implementing_receivesKeyEventsWhenFocused, focusTarget_nodeThatIsKeyInputNodeKind_delegating_receivesKeyEventsWhenFocused
#[test]
fn a_node_that_holds_a_target_gets_keys_when_that_target_has_the_focus() {
    for through_delegate in [false, true] {
        let scene = scene(move |scene| {
            Leaf(scene.req("y").then_custom(CombinedElement(Combined { through_delegate, scene })));
        });
        scene.forget();
        scene.with_host(|host| host.send_key(KeyEvent::down(Key::Space)));
        assert!(scene.said().is_empty(), "not focused: not told");
        assert!(scene.request("y"));
        scene.forget();
        scene.with_host(|host| host.send_key(KeyEvent::down(Key::Space)));
        assert_eq!(scene.said(), ["key"], "through a delegate {through_delegate}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, focusEventNodeDelegatingToFocusTarget_invalidatedOnRemoval, modifierDelegatingToFocusEventAndFocusTarget_invalidatedOnRemoval
#[test]
fn a_node_hearing_its_own_target_is_told_when_it_is_removed_with_the_focus() {
    for through_delegate in [false, true] {
        let scene = scene(move |scene| {
            if !scene.flag("gone").get() {
                Leaf(
                    scene
                        .req("y")
                        .then_custom(CombinedElement(Combined { through_delegate, scene })),
                );
            }
        });
        assert_eq!(scene.said(), ["Inactive"], "at first");
        assert!(scene.request("y"));
        scene.forget();
        scene.set("gone", true);
        assert_eq!(scene.said(), ["Inactive"], "through a delegate {through_delegate}");
    }
}
