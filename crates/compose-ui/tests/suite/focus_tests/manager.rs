//! `FocusManager::clear_focus` in each input mode, and changes asked for from inside a change.

use super::support::*;

const MODES: [InputMode; 2] = [InputMode::Touch, InputMode::Keyboard];

/// A row: what's composed, what's done first, and what clearing the focus leaves: what `x` hears
/// of it, and whether the root still has the focus.
struct Clearing {
    what: &'static str,
    content: fn(Scene),
    captures: bool,
    requests: bool,
    force: bool,
    heard: &'static [FocusState],
    root_keeps: bool,
}

fn x_alone(scene: Scene) {
    Leaf(scene.target("x"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusManagerCompositionLocalTest.kt, clearFocus_singleLayout_focusIsRestoredAfterClear, clearFocus_whenRootIsInactive, clearFocus_whenRootIsActiveParent, clearFocus_whenHierarchyHasCapturedFocus, clearFocus_forced_whenHierarchyHasCapturedFocus
#[test]
fn clearing_takes_the_focus_away_unless_it_is_captured_and_not_forced_in_each_input_mode() {
    // Compose's keyboard mode has its window give the focus back to the first target as it's
    // cleared; the window isn't a part of this (D1281), so both modes read the same here.
    let rows = [
        Clearing {
            what: "with the focus",
            content: x_alone,
            captures: false,
            requests: true,
            force: false,
            heard: &[Inactive],
            root_keeps: false,
        },
        Clearing {
            what: "with nothing focused",
            content: x_alone,
            captures: false,
            requests: false,
            force: false,
            heard: &[],
            root_keeps: false,
        },
        Clearing {
            what: "captured",
            content: x_alone,
            captures: true,
            requests: true,
            force: false,
            heard: &[],
            root_keeps: true,
        },
        Clearing {
            what: "captured, forced",
            content: x_alone,
            captures: true,
            requests: true,
            force: true,
            heard: &[Inactive],
            root_keeps: false,
        },
    ];
    for mode in MODES {
        for row in &rows {
            let scene = scene(row.content);
            scene.with_host(|host| host.set_input_mode(mode));
            scene.idle();
            if row.requests {
                assert!(scene.request("x"), "{}", row.what);
            }
            if row.captures {
                assert!(scene.capture("x"), "{}", row.what);
            }
            scene.forget();
            scene.clear(row.force);
            assert_eq!(scene.heard("x"), row.heard, "{} in {mode:?}", row.what);
            assert_eq!(scene.root_has_focus(), row.root_keeps, "{} in {mode:?}", row.what);
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusManagerCompositionLocalTest.kt, clearFocus_entireHierarchyIsCleared
#[test]
fn clearing_takes_the_focus_from_the_whole_path_in_each_input_mode() {
    for mode in MODES {
        let scene = scene(|scene| {
            Stack!(modifier = scene.target("grandparent"), {
                Stack!(modifier = scene.target("parent"), {
                    Leaf(scene.target("x"));
                });
            });
        });
        scene.with_host(|host| host.set_input_mode(mode));
        assert!(scene.request("x"));
        assert_eq!(
            scene.states(["grandparent", "parent", "x"]),
            [ActiveParent, ActiveParent, Active]
        );
        scene.clear(false);
        assert_eq!(scene.states(["grandparent", "parent", "x"]), [Inactive; 3], "{mode:?}");
    }
}

// ---- Changes from inside a change -----------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTransactionsTest.kt, reentrantRequestFocus_byCallingRequestFocusWithinOnFocusChanged, reentrantRequestFocus_byCallingRequestFocusWithinOnFocusChanged2
#[test]
fn a_request_from_a_listener_ends_where_it_asks() {
    // Asked as one target loses the focus, and as it gains it.
    for on_gaining in [false, true] {
        let armed = cell(false);
        let scene = scene(move |scene| {
            let second = scene.requester("second");
            let asks = Modifier.on_focus_changed(move |state: FocusState| {
                if state.is_focused() == on_gaining && armed.get() {
                    second.request_focus();
                }
            });
            Leaf(scene.named("first").then(asks).focus_target());
            Leaf(scene.target("second"));
        });
        if on_gaining {
            armed.set(true);
            scene.request("first");
        } else {
            assert!(scene.request("first"));
            armed.set(true);
            scene.request("second");
        }
        assert_eq!(scene.focused(), ["second"], "on gaining {on_gaining}");
        assert_eq!(scene.state("first"), Inactive);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTransactionsTest.kt, cancelTakeFocus_fromOnFocusChanged
#[test]
fn clearing_the_focus_from_a_listener_as_it_is_taken_ends_it_for_all() {
    let scene = scene(|scene| {
        let manager = scene;
        let clears = Modifier.on_focus_changed(move |state: FocusState| {
            manager.hear("middle", state);
            if state.is_focused() {
                manager.manager().clear_focus(false);
            }
        });
        Leaf(
            Modifier
                .size(10.dp())
                .then(scene.req("x"))
                .then(scene.listen("before"))
                .then(clears)
                .then(scene.listen("after"))
                .focus_target(),
        );
    });
    scene.request("x");
    assert_eq!(scene.states(["before", "middle", "after"]), [Inactive; 3]);
    assert!(!scene.root_has_focus());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTransactionsTest.kt, cancelTakeFocus_fromCustomEnter
#[test]
fn an_enter_cancelling_leaves_the_root_without_the_focus() {
    let scene = scene(|scene| {
        let cancels = properties(|p| p.set_on_enter(|scope| scope.cancel_focus_change()));
        Stack!(modifier = cancels.focus_target(), {
            Leaf(scene.req("x").focus_target());
        });
    });
    assert!(!scene.request("x"));
    assert!(!scene.root_has_focus());
}

/// What Compose's `FocusSamples.kt` relies on: a block's requests answer for real (D1270).
#[test]
fn a_request_in_a_block_acts_at_once_and_says_whether_it_did() {
    // Asked for a target there is, and for one with nothing to take the focus.
    for (name, answer, focused) in [("there", true, "there"), ("nothing", false, "second")] {
        let result = cell(None);
        let scene = scene(move |scene| {
            let asked = scene.requester(name);
            let asks = properties(move |p| {
                p.set_on_exit(move |_| {
                    let done = asked.request_focus();
                    result.set(Some(done));
                });
            });
            Stack!(modifier = asks.focus_target(), {
                Leaf(scene.target("first"));
            });
            Leaf(scene.target("second"));
            Leaf(scene.target("there"));
            Leaf(scene.req("nothing").then(cant_focus()).focus_target());
        });
        assert!(scene.request("first"));
        scene.request("second");
        assert_eq!(result.get(), Some(answer), "{name}");
        assert_eq!(scene.focused(), [focused], "{name}");
    }
}

/// As Compose's restoring sample in `FocusSamples.kt`, with nothing saved and then something.
#[test]
fn a_block_restoring_what_it_saved_enters_when_nothing_was_saved() {
    let scene = scene(|scene| {
        let group = scene.requester("group");
        let restores = properties(move |p| {
            p.set_on_exit(move |_| {
                group.save_focused_child();
            });
            p.set_on_enter(move |scope| {
                if group.restore_focused_child() {
                    scope.cancel_focus_change();
                }
            });
        });
        Column!(modifier = scene.req("group").then(restores).focus_group(), {
            Leaf(scene.target("a").size(10.dp()));
            Leaf(scene.target("b").size(10.dp()));
        });
        Leaf(scene.target("outside").size(10.dp()));
    });
    // Nothing saved: the search goes in, to the first.
    assert!(scene.move_focus(Next));
    assert_eq!(scene.focused(), ["a"]);
    // It leaves from the second, and comes back to it.
    assert!(scene.request("b"));
    assert!(scene.request("outside"));
    scene.request("a");
    assert_eq!(scene.focused(), ["b"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusManagerCompositionLocalTest.kt, clearFocus_entireHierarchyIsCleared
#[test]
fn a_clear_tells_the_group_above_even_if_a_listener_redirects_the_focus() {
    let scene = scene(|scene| {
        let was = cell(false);
        let other = scene.requester("other");
        let redirects = Modifier.on_focus_changed(move |state: FocusState| {
            if was.get() && !state.is_focused() {
                other.request_focus();
            }
            was.set(state.is_focused());
        });
        Stack!(modifier = scene.listen("group").focus_group(), {
            Leaf(scene.req("inner").then(redirects.clone()).size(10.dp()).focus_target());
        });
        Leaf(scene.target("other").size(10.dp()));
    });
    assert!(scene.request("inner"));
    scene.forget();
    scene.clear(false);
    assert_eq!(scene.focused(), ["other"]);
    assert_eq!(scene.heard("group"), [Inactive]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTransactionsTest.kt, reentrantRequestFocus_byCallingRequestFocusWithinOnFocusChanged2
#[test]
fn a_request_stops_telling_those_after_a_listener_that_moved_the_focus() {
    let scene = scene(|scene| {
        let second = scene.requester("second");
        let redirects = Modifier.on_focus_changed(move |state: FocusState| {
            if state.is_focused() {
                second.request_focus();
            }
        });
        Leaf(
            scene
                .req("first")
                .then(scene.listen("before"))
                .then(redirects)
                .then(scene.listen("after"))
                .focus_target(),
        );
        Leaf(scene.target("second"));
    });
    scene.forget();
    scene.request("first");
    assert_eq!(scene.focused(), ["second"]);
    // Told from the target up: the one above the redirect wasn't told of a focus already gone.
    assert!(!scene.heard("before").contains(&Active));
}

// ---- A Tab that can't move ----------------------------------------------------------------------------

/// Whether a Tab is consumed.
fn tab(scene: Scene) -> bool {
    scene.idle();
    let consumed = scene.with_host(|host| host.send_key(KeyEvent::down(Key::Tab)));
    scene.idle();
    consumed
}

#[test]
fn a_tab_stopped_from_moving_is_consumed_and_one_with_nowhere_to_go_isnt() {
    // Cancelled by a property.
    let scene = scene(|scene| {
        let cancels = properties(|p| p.next = FocusRequester::CANCEL);
        Leaf(scene.req("x").then(cancels).size(10.dp()).focus_target());
    });
    assert!(scene.request("x"));
    assert!(tab(*scene));

    // Refused by a capture.
    let scene = self::scene(|scene| {
        Leaf(scene.target("x").size(10.dp()));
        Leaf(scene.target("y").size(10.dp()));
    });
    assert!(scene.request("x"));
    assert!(scene.capture("x"));
    assert!(tab(*scene));
    assert_eq!(scene.focused(), ["x"]);

    // It moved.
    assert!(scene.free("x"));
    assert!(tab(*scene));
    assert_eq!(scene.focused(), ["y"]);

    // Nothing to go to.
    let scene = self::scene(|_| Leaf(Modifier.size(10.dp())));
    assert!(!tab(*scene));
}

// ---- The system's input mode -------------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/InputModeTest.kt, switchToTouchModeProgrammatically
#[test]
fn a_system_that_decides_the_input_mode_cant_be_asked_for_touch_mode() {
    let manager = cell(None);
    let scene = scene(move |_| {
        let current = LOCAL_INPUT_MODE_MANAGER.current();
        manager.set(Some(current));
    });
    let manager = manager.take().expect("composed");
    assert!(manager.request_input_mode(InputMode::Touch), "a headless host grants it");
    scene.with_host(|host| host.set_system_input_mode(InputMode::Keyboard));
    assert!(!manager.request_input_mode(InputMode::Touch));
    assert_eq!(manager.input_mode(), InputMode::Keyboard);
    scene.with_host(|host| host.set_system_input_mode(InputMode::Touch));
    assert!(manager.request_input_mode(InputMode::Touch));
    assert!(manager.request_input_mode(InputMode::Keyboard));
    assert_eq!(manager.input_mode(), InputMode::Keyboard);
}

// ---- Told once --------------------------------------------------------------------------------------------

#[test]
fn a_node_reused_tells_its_event_node_once() {
    let key = shared(0);
    let scene = scene(move |scene| {
        let at = key.get();
        compose_runtime::ReusableContent(at, move || {
            Leaf(scene.events_anew("x").size(10.dp()).focusable());
        });
    });
    scene.forget();
    key.set(1);
    scene.idle();
    assert_eq!(scene.event_states("x"), [Inactive]);
}

/// What a focus property's block reads is observed, as Compose's reads where it grants the focus.
#[test]
fn a_write_to_what_a_property_block_reads_clears_the_focus() {
    let scene = scene(|scene| {
        let gone = scene.flag("gone");
        Leaf(scene.req("x").then(properties(move |p| p.can_focus = !gone.get())).focus_target());
    });
    assert!(scene.request("x"));
    assert!(scene.root_has_focus());
    scene.set("gone", true);
    assert!(!scene.root_has_focus());
}
