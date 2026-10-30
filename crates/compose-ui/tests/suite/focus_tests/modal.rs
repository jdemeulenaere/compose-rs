//! An overlay that takes the focus is a window of its own, as Compose's dialogs are: what's behind
//! keeps its focus and takes neither keys nor searches until it closes (D1284).

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::{DisposableEffect, MutableState, ReusableContentHost, remember};

use super::support::*;

/// A target saying when a key reaches it.
fn keyed(scene: Scene, name: &'static str) -> Modifier {
    Modifier
        .on_key_event(move |event: KeyEvent| {
            let space = event.key == Key::Space;
            if space {
                scene.say(name);
            }
            space
        })
        .then(scene.target(name))
        .size(10.dp())
}

/// Who a key reaches.
fn keys_reach(scene: Scene) -> Vec<String> {
    scene.idle();
    scene.forget();
    scene.with_host(|host| host.send_key(KeyEvent::down(Key::Space)));
    scene.said()
}

fn tab(scene: Scene, shift: bool) {
    scene.idle();
    let mut event = KeyEvent::down(Key::Tab);
    event.modifiers.shift = shift;
    scene.with_host(|host| host.send_key(event));
    scene.idle();
}

/// The overlay the flag `flag` shows, its own target called `window`, as a dialog's root.
fn overlay(scene: Scene, flag: &'static str, window: &'static str, content: impl Fn() + 'static) {
    if scene.flag(flag).get() {
        compose_runtime::key(flag, || {
            Stack!(
                modifier = Modifier
                    .overlay(Rc::new(WindowPosition), true)
                    .required_size((50.dp(), 50.dp()))
                    .then(scene.listen(window))
                    .focus_target(),
                {
                    content();
                }
            );
        });
    }
}

/// Two targets behind the overlay, two in it.
fn behind_and_in(scene: Scene) {
    Column!({
        Leaf(keyed(scene, "behind"));
        Leaf(keyed(scene, "other"));
        overlay(scene, "modal", "window", move || {
            Leaf(keyed(scene, "first"));
            Leaf(keyed(scene, "second"));
        });
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedTest.kt, dispatchKeyEvent_barrierBlocksKeyEventsToOccludedFocusedNode, focusSearch_barrierDismissed_focusTargetBecomesEligibleAgain
#[test]
fn what_had_the_focus_keeps_it_behind_a_modal_and_takes_no_key_until_it_closes() {
    let scene = scene(behind_and_in);
    assert!(scene.request("behind"));
    scene.forget();
    scene.set("modal", true);
    assert_eq!(scene.state("window"), Active, "it takes the focus of its own window");
    assert_eq!(scene.state("behind"), Active, "what's behind is told nothing");
    assert!(scene.heard("behind").is_empty());
    assert!(keys_reach(*scene).is_empty(), "the window's own target hears none");

    scene.set("modal", false);
    assert!(scene.heard("behind").is_empty(), "it never lost it");
    assert_eq!(keys_reach(*scene), ["behind"]);
    assert!(scene.move_focus(Next));
    assert_eq!(scene.state("other"), Active, "searches are back");
}

#[test]
fn a_capture_behind_a_modal_survives_it() {
    let scene = scene(behind_and_in);
    assert!(scene.request("behind"));
    assert!(scene.capture("behind"));
    scene.set("modal", true);
    assert_eq!(scene.state("behind"), Captured);
    assert_eq!(scene.state("window"), Active);
    scene.set("modal", false);
    assert_eq!(scene.state("behind"), Captured);
    scene.clear(false);
    assert_eq!(scene.state("behind"), Captured, "a clear that isn't forced is refused");
    assert_eq!(keys_reach(*scene), ["behind"]);
}

#[test]
fn a_request_behind_a_modal_moves_the_focus_of_that_window_and_not_the_keys() {
    let scene = scene(behind_and_in);
    assert!(scene.request("behind"));
    scene.set("modal", true);
    assert!(scene.request("first"));

    assert!(scene.request("other"));
    assert_eq!(scene.states(["behind", "other"]), [Inactive, Active]);
    assert_eq!(scene.state("first"), Active, "the modal's window has its own");
    assert_eq!(keys_reach(*scene), ["first"]);
    scene.set("modal", false);
    assert_eq!(keys_reach(*scene), ["other"]);
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
        assert_eq!(keys_reach(*scene), ["other"]);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedTest.kt, focusSearch_barrierBlocksFocusToBackground, focusSearch_barrierDoesNotBlockFocusToChildren, activeFocusTarget_insideBarrierSubtree_retainsFocus
#[test]
fn searches_stay_in_a_modal_both_ways_and_its_own_target_is_no_stop() {
    let scene = scene(behind_and_in);
    assert!(scene.request("behind"));
    scene.set("modal", true);
    for (shift, expected) in [
        (false, "first"),
        (false, "second"),
        (false, "first"),
        (true, "second"),
        (true, "first"),
        (true, "second"),
    ] {
        tab(*scene, shift);
        assert_eq!(keys_reach(*scene), [expected], "shift {shift}");
        assert_eq!(scene.state("window"), ActiveParent);
    }
    assert!(!scene.move_focus_wrapping(Exit, true), "a window's root isn't gone to by Exit");
    assert_eq!(keys_reach(*scene), ["second"]);
    assert_eq!(scene.state("behind"), Active);
}

#[test]
fn a_modal_opening_and_closing_runs_no_block() {
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
            Stack!(modifier = told("own group").focus_target(), {
                overlay(scene, "modal", "window", move || {
                    Leaf(scene.target("first").size(10.dp()));
                });
            });
        });
    });
    scene.forget();
    scene.set("modal", true);
    assert_eq!(scene.state("window"), Active);
    assert!(scene.said().is_empty());
    scene.set("modal", false);
    assert!(scene.said().is_empty());
}

/// An effect of the overlay's content asks for the focus as it's composed.
fn asking_from_an_effect(scene: Scene) {
    Column!({
        Leaf(keyed(scene, "behind"));
        overlay(scene, "modal", "window", move || {
            Leaf(keyed(scene, "first"));
            Leaf(keyed(scene, "wanted"));
            let wanted = scene.requester("wanted");
            DisposableEffect((), move |effect| {
                wanted.request_focus();
                effect.on_dispose(|| {})
            });
        });
    });
}

#[test]
fn a_request_from_a_modals_effect_stays_and_closing_gives_the_keys_back_behind() {
    let scene = scene(asking_from_an_effect);
    assert!(scene.request("behind"));
    scene.set("modal", true);
    assert_eq!(keys_reach(*scene), ["wanted"]);
    assert_eq!(scene.state("window"), ActiveParent);
    scene.set("modal", false);
    assert_eq!(keys_reach(*scene), ["behind"]);
}

#[test]
fn a_capture_and_a_free_behind_a_modal_act_on_that_windows_focus() {
    let scene = scene(behind_and_in);
    assert!(scene.request("behind"));
    scene.set("modal", true);
    assert!(scene.capture("behind"), "it's focused, in its window");
    assert_eq!(scene.state("behind"), Captured);
    assert_eq!(scene.state("window"), Active);
    assert!(scene.free("behind"));
    assert_eq!(scene.state("behind"), Active);
    assert_eq!(scene.state("window"), Active);
    // The top window's, the other way round.
    assert!(scene.request("first"));
    assert!(scene.capture("first"));
    assert_eq!(scene.state("first"), Captured);
    assert_eq!(scene.state("behind"), Active);
    // Nothing of the capture left behind: a clear there isn't refused.
    scene.set("modal", false);
    scene.clear(false);
    assert_eq!(scene.state("behind"), Inactive);
}

#[test]
fn a_node_behind_a_modal_is_asked_its_own_windows_state() {
    let held = HeldTarget::new(Focusability::ALWAYS);
    let scene = scene(move |scene| {
        Column!({
            Leaf(held.modifier().size(10.dp()));
            overlay(scene, "modal", "window", move || Leaf(scene.target("first").size(10.dp())));
        });
    });
    scene.idle();
    assert!(held.request());
    scene.idle();
    scene.set("modal", true);
    assert!(scene.request("first"));
    assert_eq!(scene.state("first"), Active);
    assert_eq!(held.state(), Active);
    assert_eq!(held.take_changes(), [(Inactive, Active)], "told nothing since");
}

#[test]
fn a_target_losing_its_modifier_behind_a_modal_loses_the_focus_there() {
    let scene = scene(|scene| {
        Column!({
            let target = if scene.flag("gone").get() { Modifier } else { scene.target("behind") };
            Leaf(target.size(10.dp()));
            overlay(scene, "modal", "window", move || Leaf(scene.target("first").size(10.dp())));
        });
    });
    assert!(scene.request("behind"));
    scene.set("modal", true);
    assert!(scene.request("first"));
    scene.set("gone", true);
    assert_eq!(scene.state("first"), Active, "the dialog's window keeps its own");
    scene.set("modal", false);
    assert!(!scene.root_has_focus(), "the window behind has none left");
}

#[test]
fn a_clear_gives_up_the_view_only_when_no_window_has_the_focus_left() {
    let (scene, inside, outside) = dialog_scene();
    let manager = |kept: Kept| kept.borrow().as_ref().expect("composed").0.clone();
    let asked = |scene: Scene| {
        scene.idle();
        scene.with_host(|host| host.take_owner_focus_request())
    };
    // The way the view would answer: what it asked for, it hears back.
    let the_view_answers = |scene: Scene| {
        if asked(scene) == Some(false) {
            scene.with_host(|host| host.owner_lost_focus());
            scene.idle();
        }
    };

    // The dialog's first.
    assert!(scene.request("main"));
    scene.set("open", true);
    assert!(scene.request("in dialog"));
    asked(*scene);
    manager(inside).clear_focus(false);
    assert_ne!(asked(*scene), Some(false), "the main window still has it");
    assert_eq!(scene.state("main"), Active);
    the_view_answers(*scene);
    assert_eq!(scene.state("main"), Active);

    // The main window's first.
    assert!(scene.request("in dialog"));
    asked(*scene);
    manager(outside).clear_focus(false);
    assert_ne!(asked(*scene), Some(false), "the dialog's window still has it");
    assert_eq!(scene.state("in dialog"), Active);
    the_view_answers(*scene);
    assert_eq!(scene.state("in dialog"), Active);

    // The last one gives it up.
    manager(inside).clear_focus(false);
    assert_eq!(asked(*scene), Some(false));
}

/// A modal with no target, whose window has no active target of its own.
fn modal_with_no_target(scene: Scene) {
    if scene.flag("modal").get() {
        Stack!(
            modifier =
                Modifier.overlay(Rc::new(WindowPosition), true).required_size((50.dp(), 50.dp())),
            {}
        );
    }
}

/// A target behind an overlay, whose properties `properties` says in each composition.
fn behind_with(scene: Scene, properties: impl Fn(Scene) -> Modifier + 'static) {
    Column!({
        Leaf(properties(scene).then(keyed(scene, "behind")));
        modal_with_no_target(scene);
    });
}

fn loses_the_focus_behind_a_modal(scene: Scene) {
    assert!(scene.request("behind"));
    scene.set("modal", true);
    scene.set("no", true);
    assert_eq!(scene.state("behind"), Inactive, "at once, the other window over it");
    scene.set("modal", false);
    assert!(!scene.root_has_focus());
    assert!(keys_reach(scene).is_empty());
}

#[test]
fn a_properties_block_refusing_the_focus_behind_a_modal_clears_it_there() {
    let scene = scene(|scene| {
        behind_with(scene, |scene| {
            Modifier.focus_properties(move |properties| {
                properties.can_focus = !scene.flag("no").get();
            })
        });
    });
    loses_the_focus_behind_a_modal(*scene);
}

#[test]
fn an_element_changing_to_refuse_the_focus_behind_a_modal_clears_it_there() {
    let scene = scene(|scene| {
        behind_with(scene, |scene| {
            let can_focus = !scene.flag("no").get();
            Modifier.focus_properties(move |properties| properties.can_focus = can_focus)
        });
    });
    loses_the_focus_behind_a_modal(*scene);
}

#[test]
fn a_held_targets_focusability_changing_behind_a_modal_clears_it_there() {
    let held = HeldTarget::new(Focusability::ALWAYS);
    let scene = scene(move |scene| {
        Column!({
            Leaf(held.modifier().size(10.dp()));
            modal_with_no_target(scene);
        });
    });
    scene.idle();
    assert!(held.request());
    scene.idle();
    scene.set("modal", true);
    held.take_changes();
    held.set_focusability(Focusability::NEVER);
    scene.idle();
    assert_eq!(held.take_changes(), [(Active, Inactive)]);
    scene.set("modal", false);
    assert!(!scene.root_has_focus());
}

/// A node saying the focus properties changed, as it attaches.
struct InvalidatesAsItAttaches;

struct InvalidatingNode;

impl ModifierNodeElement for InvalidatesAsItAttaches {
    type Node = InvalidatingNode;
    fn create(&self) -> InvalidatingNode {
        InvalidatingNode
    }
    fn update(&self, _node: &mut InvalidatingNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "invalidates_as_it_attaches"
    }
}

impl ModifierNode for InvalidatingNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        context.invalidate_focus_properties();
    }
}

#[compose_runtime::composable]
fn attaching_on(flag: MutableState<bool>) {
    if flag.get() {
        Leaf(Modifier.then_custom(InvalidatesAsItAttaches));
    }
}

#[test]
fn a_nodes_invalidation_asks_the_window_behind_a_modal_again() {
    let refused: &'static Cell<bool> = Box::leak(Box::default());
    let scene = scene(move |scene| {
        Column!({
            let refuses =
                Modifier.focus_properties(move |properties| properties.can_focus = !refused.get());
            Leaf(refuses.then(keyed(scene, "behind")));
            // Recomposing alone: the target's chain has nothing to say.
            attaching_on(scene.flag("no"));
            modal_with_no_target(scene);
        });
    });
    assert!(scene.request("behind"));
    scene.set("modal", true);
    refused.set(true);
    scene.set("no", true);
    assert_eq!(scene.state("behind"), Inactive);
    scene.set("modal", false);
    assert!(!scene.root_has_focus());
}

#[test]
fn a_held_targets_modifier_removed_behind_a_modal_tells_it() {
    let held = HeldTarget::new(Focusability::ALWAYS);
    let scene = scene(move |scene| {
        Column!({
            let target = if scene.flag("gone").get() { Modifier } else { held.modifier() };
            Leaf(target.size(10.dp()));
            overlay(scene, "modal", "window", move || Leaf(scene.target("first").size(10.dp())));
        });
    });
    scene.idle();
    assert!(held.request());
    scene.idle();
    scene.set("modal", true);
    held.take_changes();
    scene.set("gone", true);
    assert_eq!(held.take_changes(), [(Active, Inactive)]);
    assert_eq!(scene.state("window"), Active);
}

#[test]
fn a_targets_modifier_removed_behind_a_modal_with_no_target_leaves_no_focus_when_it_closes() {
    let scene = scene(|scene| {
        Column!({
            let target = if scene.flag("gone").get() { Modifier } else { scene.target("behind") };
            Leaf(target.size(10.dp()));
            modal_with_no_target(scene);
        });
    });
    assert!(scene.request("behind"));
    scene.set("modal", true);
    scene.set("gone", true);
    scene.set("modal", false);
    assert!(!scene.root_has_focus());
}

#[test]
fn a_modal_put_aside_stops_taking_the_keys_and_takes_them_again_when_its_back() {
    let scene = scene(|scene| {
        let active = !scene.flag("aside").get();
        Column!({
            Leaf(keyed(scene, "main"));
            ReusableContentHost(active, move || {
                Stack!(
                    modifier = Modifier
                        .overlay(Rc::new(WindowPosition), true)
                        .required_size((50.dp(), 50.dp()))
                        .focus_target(),
                    {
                        Leaf(keyed(scene, "in it"));
                    }
                );
            });
        });
    });
    assert!(scene.request("main"));
    assert!(scene.request("in it"));
    assert_eq!(keys_reach(*scene), ["in it"]);
    scene.set("aside", true);
    assert_eq!(keys_reach(*scene), ["main"]);
    scene.set("aside", false);
    assert!(scene.request("in it"));
    assert_eq!(keys_reach(*scene), ["in it"]);
}

#[test]
fn a_popups_focusable_changing_keeps_its_content() {
    let composed: &'static Cell<u32> = Box::leak(Box::default());
    let inside: Kept = Box::leak(Box::default());
    let outside: Kept = Box::leak(Box::default());
    let scene = scene(move |scene| {
        *outside.borrow_mut() = Some((LOCAL_FOCUS_MANAGER.current(), LOCAL_WINDOW_INFO.current()));
        let focusable = scene.flag("focusable").get();
        Popup!(properties = PopupProperties { focusable, ..Default::default() }, {
            remember(|| composed.set(composed.get() + 1));
            *inside.borrow_mut() =
                Some((LOCAL_FOCUS_MANAGER.current(), LOCAL_WINDOW_INFO.current()));
        });
    });
    scene.idle();
    assert_eq!(composed.get(), 1);
    assert!(window_focused(outside));
    assert!(window_focused(inside));
    scene.set("focusable", true);
    assert_eq!(composed.get(), 1, "it stays");
    assert!(!window_focused(outside));
    assert!(window_focused(inside));
    scene.set("focusable", false);
    assert_eq!(composed.get(), 1, "it stays");
    assert!(window_focused(outside));
    assert!(window_focused(inside), "it's the window beneath it again");
}

/// An overlay that takes the focus until `stops` says so, as a popup's `focusable` can.
fn overlay_until(scene: Scene, stops: &'static str, content: impl Fn() + 'static) {
    Stack!(
        modifier = Modifier
            .overlay(Rc::new(WindowPosition), !scene.flag(stops).get())
            .required_size((50.dp(), 50.dp()))
            .focus_target(),
        {
            content();
        }
    );
}

#[test]
fn a_window_that_stops_taking_the_focus_loses_it_and_the_one_below_has_the_keys() {
    let scene = scene(|scene| {
        Column!({
            Leaf(keyed(scene, "main"));
            overlay_until(scene, "stops", move || Leaf(keyed(scene, "in it")));
        });
    });
    assert!(scene.request("main"));
    assert!(scene.request("in it"));
    assert_eq!(keys_reach(*scene), ["in it"]);
    scene.set("stops", true);
    assert_eq!(scene.state("in it"), Inactive);
    assert_eq!(scene.state("main"), Active);
    assert_eq!(keys_reach(*scene), ["main"]);
}

#[test]
fn a_window_under_another_that_stops_taking_the_focus_loses_it_at_once() {
    let scene = scene(|scene| {
        Column!({
            Leaf(keyed(scene, "main"));
            overlay_until(scene, "stops", move || Leaf(keyed(scene, "in lower")));
            overlay(scene, "upper", "upper window", move || Leaf(keyed(scene, "in upper")));
        });
    });
    assert!(scene.request("main"));
    assert!(scene.request("in lower"));
    scene.set("upper", true);
    assert!(scene.request("in upper"));
    scene.set("stops", true);
    assert_eq!(scene.state("in lower"), Inactive, "with the other window still over it");
    assert_eq!(keys_reach(*scene), ["in upper"]);
    scene.set("upper", false);
    assert_eq!(keys_reach(*scene), ["main"]);
}

#[test]
fn a_target_put_aside_or_removed_behind_a_modal_loses_the_focus_there() {
    let scene = scene(|scene| {
        let active = !scene.flag("aside").get();
        Column!({
            ReusableContentHost(active, move || {
                Leaf(scene.target("behind").size(10.dp()));
            });
            overlay(scene, "modal", "window", move || Leaf(scene.target("first").size(10.dp())));
        });
    });
    assert!(scene.request("behind"));
    scene.set("modal", true);
    scene.set("aside", true);
    assert_eq!(scene.state("behind"), Inactive);
    assert_eq!(scene.state("window"), Active);
    scene.set("modal", false);
    assert!(!scene.root_has_focus());

    let scene = self::scene(|scene| {
        let shown = !scene.flag("gone").get();
        Column!({
            if shown {
                Leaf(scene.target("behind").size(10.dp()));
            }
            overlay(scene, "modal", "window", move || Leaf(scene.target("first").size(10.dp())));
        });
    });
    assert!(scene.request("behind"));
    scene.set("modal", true);
    scene.set("gone", true);
    assert_eq!(scene.state("behind"), Inactive);
    scene.set("modal", false);
    assert!(!scene.root_has_focus());
}

/// Listeners added behind a modal hear their own window's state.
#[test]
fn a_listener_added_behind_a_modal_hears_that_windows_focus() {
    let scene = scene(|scene| {
        let added = scene.flag("added").get();
        Column!({
            Stack!(
                modifier = if added { scene.listen("parent") } else { Modifier }.focus_target(),
                {
                    Leaf(scene.target("behind").size(10.dp()));
                }
            );
            overlay(scene, "modal", "window", move || Leaf(scene.target("first").size(10.dp())));
        });
    });
    assert!(scene.request("behind"));
    scene.set("modal", true);
    scene.set("added", true);
    assert_eq!(scene.state("parent"), ActiveParent);
}

fn two_modals(scene: Scene) {
    Column!({
        Leaf(keyed(scene, "main"));
        overlay(scene, "lower", "lower window", move || {
            Leaf(keyed(scene, "in lower"));
            Leaf(keyed(scene, "too"));
        });
        overlay(scene, "upper", "upper window", move || {
            Leaf(keyed(scene, "in upper"));
        });
    });
}

#[test]
fn a_request_behind_two_modals_takes_the_keys_only_when_both_are_gone() {
    let scene = scene(two_modals);
    scene.set("lower", true);
    assert!(scene.request("in lower"));
    scene.set("upper", true);
    assert!(scene.request("in upper"));

    assert!(scene.request("main"));
    assert_eq!(keys_reach(*scene), ["in upper"]);
    scene.set("upper", false);
    assert_eq!(keys_reach(*scene), ["in lower"], "what had it in the window below");
    assert_eq!(scene.state("main"), Active, "in its own window");
    scene.set("lower", false);
    assert_eq!(keys_reach(*scene), ["main"]);
}

#[test]
fn a_request_in_the_lower_modal_has_the_keys_when_the_upper_one_closes() {
    let scene = scene(two_modals);
    scene.set("lower", true);
    scene.set("upper", true);
    assert!(scene.request("in upper"));
    assert!(scene.request("too"));
    assert_eq!(keys_reach(*scene), ["in upper"]);
    scene.set("upper", false);
    assert_eq!(keys_reach(*scene), ["too"]);
}

#[test]
fn the_lower_modal_closing_first_leaves_the_upper_with_the_keys_and_then_the_main_window() {
    let scene = scene(two_modals);
    assert!(scene.request("main"));
    scene.set("lower", true);
    scene.set("upper", true);
    assert!(scene.request("in upper"));
    scene.set("lower", false);
    assert_eq!(keys_reach(*scene), ["in upper"]);
    scene.set("upper", false);
    assert_eq!(keys_reach(*scene), ["main"]);
}

#[test]
fn modals_opening_together_stack_and_what_was_behind_has_the_keys_after_both() {
    let scene = scene(two_modals);
    assert!(scene.request("main"));
    scene.flag("lower").set(true);
    scene.flag("upper").set(true);
    scene.idle();
    assert_eq!(scene.state("upper window"), Active);
    assert_eq!(scene.state("main"), Active);
    scene.set("upper", false);
    assert_eq!(scene.state("lower window"), Active);
    assert!(keys_reach(*scene).is_empty(), "the lower window's own target has them");
    scene.set("lower", false);
    assert_eq!(keys_reach(*scene), ["main"]);
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
    assert_eq!(scene.state("behind"), Active);
    assert_eq!(keys_reach(*scene), ["modal"]);
}

/// The content of a dialog with a target, and what it knows of its window.
fn with_a_dialog(
    scene: Scene,
    inside: &'static std::cell::RefCell<Option<(FocusManager, WindowInfo)>>,
    outside: &'static std::cell::RefCell<Option<(FocusManager, WindowInfo)>>,
) {
    *outside.borrow_mut() = Some((LOCAL_FOCUS_MANAGER.current(), LOCAL_WINDOW_INFO.current()));
    Column!({
        Leaf(keyed(scene, "main"));
        if scene.flag("open").get() {
            Dialog!(on_dismiss_request = || {}, {
                *inside.borrow_mut() =
                    Some((LOCAL_FOCUS_MANAGER.current(), LOCAL_WINDOW_INFO.current()));
                Leaf(keyed(scene, "in dialog"));
                Leaf(keyed(scene, "too"));
            });
        }
    });
}

type Kept = &'static std::cell::RefCell<Option<(FocusManager, WindowInfo)>>;

fn dialog_scene() -> (Composed, Kept, Kept) {
    let inside: Kept = Box::leak(Box::default());
    let outside: Kept = Box::leak(Box::default());
    let scene = scene(move |scene| with_a_dialog(scene, inside, outside));
    (scene, inside, outside)
}

#[test]
fn a_dialogs_focus_manager_and_the_one_behind_it_each_clear_their_own_window() {
    let (scene, inside, outside) = dialog_scene();
    assert!(scene.request("main"));
    scene.set("open", true);
    assert!(scene.request("in dialog"));

    let manager = |kept: Kept| kept.borrow().as_ref().expect("composed").0.clone();
    manager(outside).clear_focus(false);
    scene.idle();
    assert_eq!(scene.state("main"), Inactive);
    assert_eq!(scene.state("in dialog"), Active, "the dialog's window keeps its own");

    assert!(scene.request("main"));
    manager(inside).clear_focus(false);
    scene.idle();
    assert_eq!(scene.state("in dialog"), Inactive);
    assert_eq!(scene.state("main"), Active, "the main window keeps its own");
}

#[test]
fn a_dialogs_focus_manager_moves_in_its_window_and_the_one_behind_it_in_its_own() {
    let scene = {
        let inside: Kept = Box::leak(Box::default());
        let outside: Kept = Box::leak(Box::default());
        let scene = scene(move |scene| {
            *outside.borrow_mut() =
                Some((LOCAL_FOCUS_MANAGER.current(), LOCAL_WINDOW_INFO.current()));
            Column!({
                Leaf(keyed(scene, "main"));
                Leaf(keyed(scene, "main too"));
                if scene.flag("open").get() {
                    Dialog!(on_dismiss_request = || {}, {
                        *inside.borrow_mut() =
                            Some((LOCAL_FOCUS_MANAGER.current(), LOCAL_WINDOW_INFO.current()));
                        Leaf(keyed(scene, "in dialog"));
                        Leaf(keyed(scene, "too"));
                    });
                }
            });
        });
        assert!(scene.request("main"));
        scene.set("open", true);
        assert!(scene.request("in dialog"));
        let moved = outside.borrow().as_ref().unwrap().0.clone();
        assert!(moved.move_focus(Next));
        scene.idle();
        assert_eq!(scene.state("main too"), Active);
        assert_eq!(scene.state("in dialog"), Active);
        let moved = inside.borrow().as_ref().unwrap().0.clone();
        assert!(moved.move_focus(Next));
        scene.idle();
        assert_eq!(scene.state("too"), Active);
        assert_eq!(scene.state("main too"), Active);
        // Past its last target, the window behind wraps around: the dialog's is another window's.
        let moved = outside.borrow().as_ref().unwrap().0.clone();
        assert!(moved.move_focus(Next));
        scene.idle();
        assert_eq!(scene.state("main"), Active);
        assert_eq!(scene.state("too"), Active);
        scene
    };
    assert_eq!(keys_reach(*scene), ["too"]);
}

/// Whether what kept its window sees it focused.
fn window_focused(kept: Kept) -> bool {
    kept.borrow().as_ref().expect("composed").1.is_window_focused()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/WindowInfoCompositionLocalTest.kt, mainWindowIsNotFocused_whenDialogIsVisible, windowIsFocused_whenDialogIsDismissed, windowIsFocused_onLaunch
#[test]
fn a_window_knows_whether_it_takes_the_keys() {
    let (scene, inside, outside) = dialog_scene();
    assert!(window_focused(outside));
    scene.set("open", true);
    assert!(!window_focused(outside), "the dialog's over it");
    assert!(window_focused(inside));
    scene.set("open", false);
    assert!(window_focused(outside));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/WindowInfoCompositionLocalTest.kt, mainWindowIsNotFocused_whenPopupIsVisible, windowIsFocused_whenPopupIsDismissed
#[test]
fn a_popup_that_takes_the_focus_is_a_window_and_one_that_doesnt_is_not() {
    let inside: Kept = Box::leak(Box::default());
    let outside: Kept = Box::leak(Box::default());
    let scene = scene(move |scene| {
        *outside.borrow_mut() = Some((LOCAL_FOCUS_MANAGER.current(), LOCAL_WINDOW_INFO.current()));
        for (flag, focusable) in [("focusable", true), ("not focusable", false)] {
            if scene.flag(flag).get() {
                compose_runtime::key(flag, || {
                    Popup!(properties = PopupProperties { focusable, ..Default::default() }, {
                        *inside.borrow_mut() =
                            Some((LOCAL_FOCUS_MANAGER.current(), LOCAL_WINDOW_INFO.current()));
                    });
                });
            }
        }
    });

    scene.set("focusable", true);
    assert!(!window_focused(outside), "the popup's over it");
    assert!(window_focused(inside));
    scene.set("focusable", false);
    assert!(window_focused(outside));

    scene.set("not focusable", true);
    assert!(window_focused(outside), "it takes no keys");
    assert!(window_focused(inside), "it's in the window beneath it");
    scene.set("not focusable", false);
    assert!(window_focused(outside));
}
