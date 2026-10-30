//! A modal overlay: a window of its own in Compose, its barrier for what's behind it here (D1269).

use std::rc::Rc;

use compose_runtime::{DisposableEffect, ReusableContentHost};

use super::support::*;

/// The overlay the flag `modal` shows, a target of its own, as a dialog's window has its root.
fn overlay(scene: Scene, content: impl Fn() + 'static) {
    if scene.flag("modal").get() {
        Stack!(
            modifier = Modifier
                .overlay(Rc::new(WindowPosition), true)
                .required_size((50.dp(), 50.dp()))
                .then(scene.listen("window"))
                .focus_target(),
            {
                content();
            }
        );
    }
}

/// Two targets behind the overlay, two in it.
fn behind_and_in(scene: Scene) {
    Column!({
        Leaf(scene.target("behind").size(10.dp()));
        Leaf(scene.target("other").size(10.dp()));
        overlay(scene, move || {
            Leaf(scene.target("first").size(10.dp()));
            Leaf(scene.target("second").size(10.dp()));
        });
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedTest.kt, requestFocus_rejectedWhenTargetIsOccludedByBarrier, focusSearch_barrierDismissed_focusTargetBecomesEligibleAgain, activeFocusTarget_barrierDismissed_targetCanBeRefocused
#[test]
fn a_request_behind_a_modal_changes_nothing_until_it_closes_then_it_has_the_focus() {
    let scene = scene(behind_and_in);
    assert!(scene.request("behind"));
    scene.set("modal", true);
    assert_eq!(scene.focused(), ["window"]);

    assert!(scene.request("other"), "the answer is yes: it's what a dialog's window would say");
    assert_eq!(scene.focused(), ["window"], "what's behind doesn't get it while it shows");
    scene.set("modal", false);
    assert_eq!(scene.focused(), ["other"], "not what had it before");
}

#[test]
fn a_request_behind_a_closing_modal_is_kept_in_either_order() {
    for close_first in [false, true] {
        let scene = scene(behind_and_in);
        scene.set("modal", true);
        if close_first {
            scene.flag("modal").set(false);
            assert!(scene.requester("other").request_focus());
        } else {
            assert!(scene.requester("other").request_focus());
            scene.flag("modal").set(false);
        }
        scene.idle();
        assert_eq!(scene.focused(), ["other"], "closed first {close_first}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedTest.kt, focusSearch_barrierBlocksFocusToBackground, focusSearch_barrierDoesNotBlockFocusToChildren, activeFocusTarget_insideBarrierSubtree_retainsFocus
#[test]
fn searches_stay_in_a_modal_both_ways_and_its_own_target_is_no_stop() {
    let scene = scene(behind_and_in);
    scene.set("modal", true);
    assert_eq!(scene.focused(), ["window"], "it takes the focus as it opens");
    for (direction, expected) in [
        (Next, "first"),
        (Next, "second"),
        (Next, "first"),
        (Previous, "second"),
        (Previous, "first"),
        (Previous, "second"),
    ] {
        assert!(scene.move_focus(direction));
        assert_eq!(scene.focused(), [expected], "{direction:?}");
        assert_eq!(scene.state("window"), ActiveParent);
    }
    assert!(!scene.move_focus(Exit), "a window's root isn't gone to by Exit");
    assert_eq!(scene.focused(), ["second"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedTest.kt, dispatchKeyEvent_barrierBlocksKeyEventsToOccludedFocusedNode, activeFocusTarget_clearsFocusWhenBarrierPlacedOnTop
#[test]
fn a_modal_opening_takes_the_focus_from_a_captured_target_and_no_key_reaches_it() {
    let scene = scene(|scene| {
        Column!({
            Leaf(scene.target("behind").size(10.dp()).on_key_event(move |_: KeyEvent| {
                scene.say("key");
                true
            }));
            overlay(scene, move || Leaf(scene.target("first").size(10.dp())));
        });
    });
    assert!(scene.request("behind"));
    assert!(scene.capture("behind"));
    scene.set("modal", true);
    assert_eq!(scene.focused(), ["window"], "the tree's move forces what a capture would refuse");
    scene.forget();
    scene.with_host(|host| host.send_key(KeyEvent::down(Key::Space)));
    assert!(scene.said().is_empty());
}

#[test]
fn a_modal_opening_and_closing_runs_no_block_of_the_targets_it_moves_between() {
    let scene = scene(|scene| {
        let told = move |what: &'static str| {
            properties(move |p| {
                p.set_on_enter(move |scope| {
                    scene.say(format!("enter {what}"));
                    scope.cancel_focus_change();
                });
                p.set_on_exit(move |scope| {
                    scene.say(format!("exit {what}"));
                    scope.cancel_focus_change();
                });
            })
        };
        Column!({
            Stack!(modifier = told("group").focus_target(), {
                Leaf(scene.target("behind").size(10.dp()));
            });
            overlay(scene, move || Leaf(scene.target("first").size(10.dp())));
        });
    });
    scene.requester("behind").request_focus();
    scene.idle();
    // Its group's enter cancelled it: asked from within the tree it would still be taken.
    assert!(scene.focused().is_empty());
    scene.set("modal", true);
    assert_eq!(scene.focused(), ["window"]);
    scene.forget();
    scene.set("modal", false);
    assert!(scene.said().is_empty());
}

/// An effect of the overlay's content asks for the focus as it's composed.
fn asking_from_an_effect(scene: Scene) {
    Column!({
        Leaf(scene.target("behind").size(10.dp()));
        overlay(scene, move || {
            Leaf(scene.target("first").size(10.dp()));
            Leaf(scene.target("wanted").size(10.dp()));
            let wanted = scene.requester("wanted");
            DisposableEffect((), move |effect| {
                wanted.request_focus();
                effect.on_dispose(|| {})
            });
        });
    });
}

#[test]
fn a_request_from_a_modals_effect_stays_and_closing_gives_the_focus_back_behind() {
    let scene = scene(asking_from_an_effect);
    assert!(scene.request("behind"));
    scene.set("modal", true);
    assert_eq!(scene.focused(), ["wanted"]);
    scene.set("modal", false);
    assert_eq!(scene.focused(), ["behind"]);
}

#[test]
fn closing_a_modal_doesnt_give_the_focus_to_a_target_put_aside_meanwhile() {
    let scene = scene(|scene| {
        let active = !scene.flag("aside").get();
        Column!({
            ReusableContentHost(active, move || {
                Leaf(scene.target("behind").size(10.dp()));
            });
            overlay(scene, move || Leaf(scene.target("first").size(10.dp())));
        });
    });
    assert!(scene.request("behind"));
    scene.set("modal", true);
    scene.set("aside", true);
    scene.set("modal", false);
    assert!(!scene.root_has_focus());
}

#[test]
fn closing_a_modal_doesnt_give_the_focus_to_a_target_removed_meanwhile() {
    let scene = scene(|scene| {
        let shown = !scene.flag("gone").get();
        Column!({
            if shown {
                Leaf(scene.target("behind").size(10.dp()));
            }
            overlay(scene, move || Leaf(scene.target("first").size(10.dp())));
        });
    });
    assert!(scene.request("behind"));
    scene.set("modal", true);
    scene.set("gone", true);
    scene.set("modal", false);
    assert!(!scene.root_has_focus());
}

#[test]
fn a_request_behind_two_modals_waits_for_both_to_close() {
    let scene = scene(|scene| {
        Column!({
            Leaf(scene.target("main").size(10.dp()));
            if scene.flag("lower").get() {
                Stack!(
                    modifier = Modifier
                        .overlay(Rc::new(WindowPosition), true)
                        .required_size((50.dp(), 50.dp()))
                        .then(scene.listen("lower window"))
                        .focus_target(),
                    {
                        Leaf(scene.target("in lower").size(10.dp()));
                    }
                );
            }
            if scene.flag("upper").get() {
                Stack!(
                    modifier = Modifier
                        .overlay(Rc::new(WindowPosition), true)
                        .required_size((50.dp(), 50.dp()))
                        .then(scene.listen("upper window"))
                        .focus_target(),
                    {
                        Leaf(scene.target("in upper").size(10.dp()));
                    }
                );
            }
        });
    });
    scene.set("lower", true);
    assert_eq!(scene.focused(), ["lower window"]);
    assert!(scene.request("in lower"));
    scene.set("upper", true);
    assert_eq!(scene.focused(), ["upper window"]);

    assert!(scene.request("main"));
    assert_eq!(scene.focused(), ["upper window"]);
    scene.set("upper", false);
    assert_eq!(scene.focused(), ["in lower"], "what had it in the window below");
    scene.set("lower", false);
    assert_eq!(scene.focused(), ["main"]);
}

#[test]
fn a_request_in_the_lower_modal_has_the_focus_when_the_upper_one_closes() {
    let scene = scene(|scene| {
        Column!({
            if scene.flag("lower").get() {
                Stack!(
                    modifier = Modifier
                        .overlay(Rc::new(WindowPosition), true)
                        .required_size((50.dp(), 50.dp())),
                    {
                        Leaf(scene.target("in lower").size(10.dp()));
                        Leaf(scene.target("too").size(10.dp()));
                    }
                );
            }
            if scene.flag("upper").get() {
                Stack!(
                    modifier = Modifier
                        .overlay(Rc::new(WindowPosition), true)
                        .required_size((50.dp(), 50.dp())),
                    {
                        Leaf(scene.target("in upper").size(10.dp()));
                    }
                );
            }
        });
    });
    scene.set("lower", true);
    scene.set("upper", true);
    assert_eq!(scene.focused(), ["in upper"]);
    assert!(scene.request("too"));
    assert_eq!(scene.focused(), ["in upper"]);
    scene.set("upper", false);
    assert_eq!(scene.focused(), ["too"]);
}

#[test]
fn keys_go_to_a_modal_with_no_target_rather_than_what_it_shuts_out() {
    let scene = scene(|scene| {
        let hears = move |what: &'static str| {
            Modifier.on_key_event(move |_: KeyEvent| {
                scene.say(what);
                true
            })
        };
        Column!({
            Leaf(hears("behind").then(scene.target("behind")).size(10.dp()));
            if scene.flag("modal").get() {
                Leaf(
                    hears("modal")
                        .overlay(Rc::new(WindowPosition), true)
                        .required_size((50.dp(), 50.dp())),
                );
            }
        });
    });
    assert!(scene.request("behind"));
    scene.set("modal", true);
    assert_eq!(scene.focused(), ["behind"], "it took nothing: nothing in it to take it");
    scene.forget();
    scene.with_host(|host| host.send_key(KeyEvent::down(Key::Space)));
    assert_eq!(scene.said(), ["modal"]);
}

#[test]
fn a_modal_closing_with_another_still_showing_leaves_what_it_covered_for_the_other() {
    let scene = scene(|scene| {
        Column!({
            Leaf(scene.target("behind").size(10.dp()));
            if scene.flag("both").get() {
                for name in ["lower", "upper"] {
                    if name == "lower" || !scene.flag("upper gone").get() {
                        Stack!(
                            modifier = Modifier
                                .overlay(Rc::new(WindowPosition), true)
                                .required_size((50.dp(), 50.dp()))
                                .then(scene.listen(name))
                                .focus_target(),
                            {}
                        );
                    }
                }
            }
        });
    });
    assert!(scene.request("behind"));
    // Opening together, each keeps what had it behind.
    scene.set("both", true);
    assert_eq!(scene.state("behind"), Inactive);
    scene.set("upper gone", true);
    assert_eq!(scene.state("behind"), Inactive, "the lower one still shuts it out");
    scene.set("both", false);
    assert_eq!(scene.focused(), ["behind"]);
}
