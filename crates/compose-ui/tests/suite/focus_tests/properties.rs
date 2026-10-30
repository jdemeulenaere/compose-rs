//! `focus_properties`: where a move goes, cancelling it, what `on_enter` and `on_exit` do to a
//! change, and a target's rectangle.

use super::support::*;

/// Sets `direction`'s destination.
fn set(p: &mut FocusProperties, direction: FocusDirection, to: FocusRequester) {
    match direction {
        Left => p.left = to,
        Right => p.right = to,
        Up => p.up = to,
        Down => p.down = to,
        Next => p.next = to,
        Previous => p.previous = to,
        _ => unreachable!("no property"),
    }
}

// ---- Cancelling a move ------------------------------------------------------------------------------

/// A parent over three rows of three, the one in the middle over a child; `center` on the middle
/// one's chain.
fn three_by_three(scene: Scene, center: Modifier) {
    let item = move |name| Modifier.size(10.dp()).then(scene.target(name));
    Column!(modifier = Modifier.size(100.dp()).then(scene.target("parent")), {
        Row!({
            Leaf(item("a"));
            Leaf(item("above"));
            Leaf(item("c"));
        });
        let center = center.clone();
        Row!({
            Leaf(item("before"));
            let center = center.clone();
            Stack!(modifier = center.size(10.dp()).then(scene.target("center")), {
                Leaf(Modifier.size(10.dp()).then(scene.target("child")));
            });
            Leaf(item("after"));
        });
        Row!({
            Leaf(item("g"));
            Leaf(item("below"));
            Leaf(item("i"));
        });
    });
}

const EVERY_MOVE: [(FocusDirection, &str); 8] = [
    (Left, "before"),
    (Right, "after"),
    (Up, "above"),
    (Down, "below"),
    (Enter, "child"),
    (Exit, "parent"),
    (Previous, "before"),
    (Next, "child"),
];

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CancelFocusMoveTest.kt, defaultOrderIsUsed, cancelLeft, cancelRight, cancelUp, cancelDown, cancelNext, cancelPrevious, cancelNextAndPrevious, cancelLeftAndRight, cancelAllDirections
#[test]
fn a_direction_set_to_cancel_stops_only_that_move() {
    const CANCELLED: [&[FocusDirection]; 10] = [
        &[],
        &[Left],
        &[Right],
        &[Up],
        &[Down],
        &[Next],
        &[Previous],
        &[Next, Previous],
        &[Left, Right],
        &[Left, Right, Up, Down, Previous, Next],
    ];
    for cancelled in CANCELLED {
        for (direction, default_result) in EVERY_MOVE {
            let scene = scene(move |scene| {
                let cancels = properties(move |p| {
                    for &each in cancelled {
                        set(p, each, FocusRequester::CANCEL);
                    }
                });
                three_by_three(scene, cancels);
            });
            assert!(scene.request("center"));
            let moved = scene.move_focus(direction);
            let stops = cancelled.contains(&direction);
            assert_eq!(moved, !stops, "{direction:?} with {cancelled:?} cancelled");
            let expected = if stops { "center" } else { default_result };
            assert_eq!(scene.focused(), [expected], "{direction:?} with {cancelled:?} cancelled");
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CancelFocusMoveTest.kt, cancelRight_clearedByParent
#[test]
fn a_cancel_is_overruled_by_a_default_earlier_in_the_chain() {
    for (direction, default_result) in EVERY_MOVE {
        let scene = scene(move |scene| {
            let chain = properties(|p| p.right = FocusRequester::DEFAULT)
                .then(properties(|p| p.right = FocusRequester::CANCEL));
            three_by_three(scene, chain);
        });
        assert!(scene.request("center"));
        assert!(scene.move_focus(direction), "{direction:?}");
        assert_eq!(scene.focused(), [default_result], "{direction:?}");
    }
}

// ---- Custom destinations, by the API and by keys -------------------------------------------------------

/// How a move is asked for.
#[derive(Clone, Copy, Debug)]
enum Arm {
    Programmatic,
    /// With a key press. Only `Next` and `Previous` have keys so far: the other directions wait
    /// for `key-focus-navigation`, and D230 keeps them out of `dispatch_key`.
    Key,
}

const ARMS: [Arm; 2] = [Arm::Programmatic, Arm::Key];

/// Asks for a move, in `arm`. Whether the arm could ask for it.
fn ask(scene: Scene, arm: Arm, direction: FocusDirection) -> bool {
    match (arm, direction) {
        (Arm::Programmatic, _) => {
            scene.move_focus(direction);
            true
        }
        (Arm::Key, Next | Previous) => {
            let mut event = KeyEvent::down(Key::Tab);
            event.modifiers.shift = direction == Previous;
            scene.idle();
            scene.with_host(|host| host.send_key(event));
            scene.idle();
            true
        }
        (Arm::Key, _) => false,
    }
}

/// Three targets on a line along `direction`, a property on the first one that says to go to the
/// last, or the other way.
fn line_with_a_shortcut(
    scene: Scene,
    vertical: bool,
    property: fn(&mut FocusProperties, FocusRequester),
    on_first: bool,
) {
    let to = scene.requester(if on_first { "third" } else { "first" });
    let shortcut = properties(move |p| property(p, to));
    let content = move || {
        let item = move |name| Modifier.size(10.dp()).then(scene.target(name));
        Leaf(if on_first { shortcut.clone().then(item("first")) } else { item("first") });
        Leaf(item("second"));
        Leaf(if on_first { item("third") } else { shortcut.clone().then(item("third")) });
    };
    if vertical {
        Column!({ content() });
    } else {
        Row!({ content() });
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CustomFocusTraversalTest.kt, focusProperties_next, focusProperties_previous, focusProperties_up, focusProperties_down, focusProperties_left, focusProperties_right, focusProperties_start, focusProperties_end
#[test]
fn a_direction_set_to_a_requester_moves_there_past_what_is_nearer() {
    // (the property, on a column, from the first, the direction)
    let rows: [(fn(&mut FocusProperties, FocusRequester), bool, bool, FocusDirection); 8] = [
        (|p, to| p.next = to, false, true, Next),
        (|p, to| p.previous = to, false, false, Previous),
        (|p, to| p.up = to, true, false, Up),
        (|p, to| p.down = to, true, true, Down),
        (|p, to| p.left = to, false, false, Left),
        (|p, to| p.right = to, false, true, Right),
        (|p, to| p.start = to, false, false, Left),
        (|p, to| p.end = to, false, true, Right),
    ];
    for arm in ARMS {
        for (property, vertical, on_first, direction) in rows {
            let scene =
                scene(move |scene| line_with_a_shortcut(scene, vertical, property, on_first));
            let (from, to) = if on_first { ("first", "third") } else { ("third", "first") };
            assert!(scene.request(from));
            if ask(*scene, arm, direction) {
                assert_eq!(scene.focused(), [to], "{direction:?}, {arm:?}");
            }
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CustomFocusTraversalTest.kt, focusProperties_enter, focusProperties_exit
#[test]
fn an_enter_is_told_its_direction_and_can_send_the_move_elsewhere() {
    // On a parent with children, and on a target with none of its own under a parent.
    for own_children in [true, false] {
        let heard = cell(None);
        let scene = scene(move |scene| {
            let there = scene.requester("third");
            let chooses = properties(move |p| {
                p.set_on_enter(move |scope| {
                    heard.set(Some(scope.requested_focus_direction()));
                    there.request_focus();
                })
            });
            let item = move |name| Modifier.size(10.dp()).then(scene.target(name));
            if own_children {
                Row!(modifier = scene.req("start").then(chooses).focus_target(), {
                    Leaf(item("first"));
                    Leaf(item("second"));
                    Leaf(item("third"));
                });
            } else {
                Row!(modifier = scene.listen("parent").focus_target(), {
                    Leaf(chooses.clone().then(scene.req("start")).then(item("first")));
                    Leaf(item("second"));
                    Leaf(item("third"));
                });
            }
        });
        assert!(scene.request("start"));
        ask(*scene, Arm::Programmatic, Enter);
        assert_eq!(heard.get(), Some(Enter));
        assert_eq!(scene.focused(), ["third"]);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CustomFocusTraversalTest.kt, focusProperties_outermostParentWins, focusProperties_parentCanResetCustomNextSetByChild, focusProperties_emptyFocusPropertiesInParent_doesNotResetCustomNextSetByChild
#[test]
fn properties_above_a_target_are_applied_after_its_own_outermost_last() {
    // What the parent's says of `next`: and where it ends.
    let rows: [(fn(Scene, &mut FocusProperties), &str); 3] = [
        (|scene, p| p.next = scene.requester("fourth"), "fourth"),
        (|_, p| p.next = FocusRequester::DEFAULT, "second"),
        (|_, _| {}, "third"),
    ];
    for arm in ARMS {
        for (parent_says, expected) in rows {
            let scene = scene(move |scene| {
                Row!({
                    Stack!(modifier = properties(move |p| parent_says(scene, p)), {
                        let to = scene.requester("third");
                        Leaf(
                            Modifier
                                .size(10.dp())
                                .then(properties(move |p| p.next = to))
                                .then(scene.target("first")),
                        );
                    });
                    for name in ["second", "third", "fourth"] {
                        Leaf(Modifier.size(10.dp()).then(scene.target(name)));
                    }
                });
            });
            assert!(scene.request("first"));
            ask(*scene, arm, Next);
            assert_eq!(scene.focused(), [expected], "{arm:?}");
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CustomFocusTraversalTest.kt, changedFocusProperties, changedFocusProperties_mutableState
#[test]
fn a_destination_changed_after_composing_is_the_one_a_move_uses() {
    for arm in ARMS {
        // Whether the change is to a plain value or to a state.
        for as_state in [false, true] {
            let plain = cell(false);
            let scene = scene(move |scene| {
                let (third, fourth) = (scene.requester("third"), scene.requester("fourth"));
                let flag = scene.flag("fourth");
                let to = properties(move |p| {
                    let changed = if as_state { flag.get() } else { plain.get() };
                    p.next = if changed { fourth } else { third };
                });
                Row!({
                    Leaf(Modifier.size(10.dp()).then(to.clone()).then(scene.target("first")));
                    for name in ["second", "third", "fourth"] {
                        Leaf(Modifier.size(10.dp()).then(scene.target(name)));
                    }
                });
            });
            assert!(scene.request("first"));
            if as_state {
                scene.set("fourth", true);
            } else {
                plain.set(true);
            }
            ask(*scene, arm, Next);
            assert_eq!(scene.focused(), ["fourth"], "{arm:?}, state {as_state}");
        }
    }
}

/// On a line, the first two with a shortcut of a direction a target doesn't move in, in a table of
/// each direction's `set`: the requester asked for, past what the direction would find.
#[test]
fn a_requester_set_for_a_direction_answers_for_that_direction_alone() {
    for direction in [Left, Right, Up, Down, Next, Previous] {
        for (asked, expected) in [(direction, "far"), (other_than(direction), "near")] {
            let scene = scene(move |scene| {
                let to = scene.requester("far");
                let shortcut = properties(move |p| set(p, direction, to));
                three_by_three(scene, shortcut);
                Leaf(Modifier.size(10.dp()).then(scene.target("far")));
            });
            assert!(scene.request("center"));
            scene.move_focus(asked);
            let moved_to = scene.focused();
            if expected == "far" {
                assert_eq!(moved_to, ["far"], "{direction:?}");
            } else {
                assert_ne!(moved_to, ["far"], "{direction:?} asked {asked:?}");
            }
        }
    }
}

fn other_than(direction: FocusDirection) -> FocusDirection {
    match direction {
        Left => Right,
        Right => Left,
        Up => Down,
        Down => Up,
        Next => Previous,
        _ => Next,
    }
}

// ---- on_exit, when the focus is cleared -----------------------------------------------------------------

/// Where the `on_exit` of a clear is, around the target `x`.
#[derive(Clone, Copy)]
enum Place {
    Itself,
    Child,
    Parent,
    Grandparent,
}

/// `x` with `exit` at `place`.
fn with_block_at(scene: Scene, place: Place, block: Modifier) {
    let x = scene.target("x");
    match place {
        Place::Itself => Leaf(block.then(x)),
        Place::Child => {
            Stack!(modifier = x, {
                Leaf(block.clone().focus_target());
            });
        }
        Place::Parent => {
            Stack!(modifier = block.focus_target(), {
                Leaf(x.clone());
            });
        }
        Place::Grandparent => {
            Stack!(modifier = block.focus_target(), {
                let x = x.clone();
                Stack!({
                    Leaf(x.clone());
                });
            });
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ClearFocusExitTest.kt, clearFocus_doesNotTriggerExit, clearFocus_doesNotTriggersExitForChild, clearFocus_triggersExitForParent, clearFocus_triggersExitForGrandparent
#[test]
fn clearing_the_focus_runs_the_exit_of_the_ancestors_only() {
    for (place, runs) in [
        (Place::Itself, false),
        (Place::Child, false),
        (Place::Parent, true),
        (Place::Grandparent, true),
    ] {
        let ran = cell(false);
        let scene = scene(move |scene| {
            with_block_at(scene, place, properties(move |p| p.set_on_exit(move |_| ran.set(true))));
        });
        assert!(scene.request("x"));
        scene.clear(false);
        assert_eq!(ran.get(), runs);
        assert_eq!(scene.state("x"), Inactive);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ClearFocusExitTest.kt, cancellingClearFocus_usingExitProperty, cancellingClearFocus_usingExitPropertyOnChild, cancellingClearFocus_usingExitPropertyOnParent, cancellingClearFocus_usingExitPropertyOnGrandparent
#[test]
fn an_ancestors_exit_can_cancel_a_clear_and_the_targets_own_or_a_childs_cant() {
    for (place, keeps) in [
        (Place::Itself, false),
        (Place::Child, false),
        (Place::Parent, true),
        (Place::Grandparent, true),
    ] {
        let scene = scene(move |scene| {
            let cancels = properties(|p| p.set_on_exit(|scope| scope.cancel_focus_change()));
            with_block_at(scene, place, cancels);
        });
        assert!(scene.request("x"));
        scene.clear(false);
        assert_eq!(scene.state("x").is_focused(), keeps);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ClearFocusExitTest.kt, clearFocus_usingChildAsCustomExitDestination, clearFocus_usingSelfAsCustomExitDestination, clearFocus_usingSiblingAsCustomExitDestination
#[test]
fn an_ancestors_exit_can_send_a_clear_elsewhere() {
    fn exit_to(scene: Scene, name: &'static str) -> Modifier {
        let there = scene.requester(name);
        properties(move |p| {
            p.set_on_exit(move |_| {
                there.request_focus();
            })
        })
    }
    let rows: [(&str, fn(Scene)); 3] = [
        ("a child", |scene| {
            Stack!(modifier = exit_to(scene, "there").focus_target(), {
                Leaf(scene.req("x").focus_target());
                Leaf(scene.target("there"));
            });
        }),
        ("itself", |scene| {
            Stack!(modifier = exit_to(scene, "there").then(scene.target("there")), {
                Leaf(scene.req("x").focus_target());
            });
        }),
        ("its sibling", |scene| {
            Stack!({
                Stack!(modifier = exit_to(scene, "there").focus_target(), {
                    Leaf(scene.req("x").focus_target());
                });
                Leaf(scene.target("there"));
            });
        }),
    ];
    for (what, content) in rows {
        let scene = scene(content);
        assert!(scene.request("x"));
        scene.clear(false);
        assert_eq!(scene.focused(), ["there"], "{what}");
    }
}

// ---- on_enter, when a target takes the focus -------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusEnterTest.kt, gainingFocus_doesNotTriggersEnter, gainingFocus_doesNotTriggersEnterForChild, gainingFocus_triggersEnterForParent, gainingFocus_triggersEnterForGrandparent
#[test]
fn taking_the_focus_runs_the_enter_of_the_ancestors_only() {
    for (place, runs) in [
        (Place::Itself, false),
        (Place::Child, false),
        (Place::Parent, true),
        (Place::Grandparent, true),
    ] {
        let ran = cell(false);
        let scene = scene(move |scene| {
            with_block_at(
                scene,
                place,
                properties(move |p| p.set_on_enter(move |_| ran.set(true))),
            );
        });
        assert!(scene.request("x"));
        assert_eq!(ran.get(), runs);
        assert_eq!(scene.state("x"), Active);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusEnterTest.kt, cancellingFocusGain_usingEnterProperty, cancellingFocusGain_usingEnterPropertyOnChild, cancellingFocusGain_usingEnterPropertyOnParent, cancellingFocusGain_usingEnterPropertyOnGrandparent
#[test]
fn an_ancestors_enter_can_cancel_taking_the_focus_and_the_targets_own_or_a_childs_cant() {
    for (place, takes) in [
        (Place::Itself, true),
        (Place::Child, true),
        (Place::Parent, false),
        (Place::Grandparent, false),
    ] {
        let scene = scene(move |scene| {
            let cancels = properties(|p| p.set_on_enter(|scope| scope.cancel_focus_change()));
            with_block_at(scene, place, cancels);
        });
        scene.request("x");
        assert_eq!(scene.state("x").is_focused(), takes);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusEnterTest.kt, cancellingFocusGain_usingACustomDestination, cancellingFocusGain_usingAChildAsACustomDestination, cancellingFocusGain_usingASiblingAsACustomDestination, cancellingFocusGain_usingParentAsACustomDestination
#[test]
fn an_ancestors_enter_can_send_a_request_elsewhere() {
    fn enter_to(scene: Scene, name: &'static str) -> Modifier {
        let there = scene.requester(name);
        properties(move |p| {
            p.set_on_enter(move |_| {
                there.request_focus();
            })
        })
    }
    let rows: [(&str, fn(Scene)); 4] = [
        ("under another parent", |scene| {
            Stack!(modifier = Modifier.focus_target(), {
                Stack!(modifier = enter_to(scene, "there").focus_target(), {
                    Leaf(scene.target("x"));
                    Leaf(Modifier.focus_target());
                });
                Stack!(modifier = Modifier.focus_target(), {
                    Leaf(scene.target("there"));
                });
            });
        }),
        ("a child", |scene| {
            Stack!(modifier = enter_to(scene, "there").focus_target(), {
                Leaf(scene.target("x"));
                Leaf(scene.target("there"));
            });
        }),
        ("its sibling", |scene| {
            Stack!(modifier = enter_to(scene, "there").focus_target(), {
                Leaf(scene.target("x"));
            });
            Leaf(scene.target("there"));
        }),
        ("itself", |scene| {
            Stack!(modifier = enter_to(scene, "there").then(scene.target("there")), {
                Leaf(scene.target("x"));
            });
        }),
    ];
    for (what, content) in rows {
        let scene = scene(content);
        scene.request("x");
        assert_eq!(scene.focused(), ["there"], "{what}");
    }
}

/// Two children of a parent whose block goes to the second, and another target beside it, the
/// block counted.
fn redirecting_to_second(scene: Scene, count: &'static std::cell::Cell<i32>, on_exit: bool) {
    let there = scene.requester("second");
    let redirects = properties(move |p| {
        let block = move |_: &mut FocusEnterExitScope| {
            count.set(count.get() + 1);
            there.request_focus();
        };
        if on_exit {
            p.set_on_exit(block);
        } else {
            p.set_on_enter(block);
        }
    });
    Stack!(modifier = Modifier.focus_target(), {
        Leaf(scene.target("beside"));
        let redirects = redirects.clone();
        Stack!(modifier = redirects.focus_target(), {
            Leaf(scene.target("first"));
            Leaf(scene.target("second"));
        });
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusEnterTest.kt, redirectingFocusRequestOnChild1ToChild2_focusEnterIsCalled
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusExitTest.kt, redirectingFocusExitFromChild1ToChild2_focusExitIsCalled
#[test]
fn a_block_redirecting_within_its_own_parent_runs_once_for_each_time_the_focus_crosses_it() {
    let count = cell(0);
    let scene = scene(move |scene| redirecting_to_second(scene, count, false));
    assert!(scene.request("beside"));
    scene.request("first");
    assert_eq!(count.get(), 1);
    assert_eq!(scene.focused(), ["second"]);
    scene.request("beside");
    assert_eq!(count.get(), 1, "leaving isn't entering");
    scene.request("second");
    assert_eq!(count.get(), 2);
    assert_eq!(scene.focused(), ["second"]);

    let count = cell(0);
    let scene = self::scene(move |scene| redirecting_to_second(scene, count, true));
    assert!(scene.request("first"));
    scene.request("beside");
    assert_eq!(count.get(), 1);
    assert_eq!(scene.focused(), ["second"], "kept in, on the sibling");
    assert!(scene.request("second"));
    assert_eq!(count.get(), 1, "moving within isn't exiting");
    // Asking for what already has the focus redirects nothing: the move goes on.
    scene.request("beside");
    assert_eq!(count.get(), 2);
    assert_eq!(scene.focused(), ["beside"]);
    assert!(scene.request("first"));
    count.set(0);
    scene.request("beside");
    assert_eq!(count.get(), 1);
}

// ---- Which blocks can stop a move across the tree ---------------------------------------------------------

/// Moves from `child1` to `child4`, the block of `cancel` (a name, and whether it's its enter)
/// cancelling: the order of the blocks that ran, and who has the focus.
fn move_between_cousins(cancel: Option<(&'static str, bool)>) -> (Vec<String>, Vec<&'static str>) {
    let armed = cell(false);
    let scene = scene(move |scene| {
        let block = move |name: &'static str| {
            properties(move |p| {
                p.set_on_enter(move |scope| {
                    if armed.get() {
                        scene.say(format!("enter {name}"));
                        if cancel == Some((name, true)) {
                            scope.cancel_focus_change();
                        }
                    }
                });
                p.set_on_exit(move |scope| {
                    if armed.get() {
                        scene.say(format!("exit {name}"));
                        if cancel == Some((name, false)) {
                            scope.cancel_focus_change();
                        }
                    }
                });
            })
        };
        let target = move |name: &'static str| block(name).then(scene.target(name));
        Stack!(modifier = target("grandparent"), {
            Stack!(modifier = target("parent1"), {
                Leaf(target("child1"));
                Leaf(target("child2"));
            });
            Stack!(modifier = target("parent2"), {
                Leaf(target("child3"));
                Leaf(target("child4"));
            });
        });
    });
    assert!(scene.request("child1"));
    scene.forget();
    armed.set(true);
    scene.request("child4");
    (scene.said(), scene.focused())
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusEnterExitTest.kt, triggersExitAndEnter, exitPropertyOnFocusedItem_cantStopFocusFromLeaving, exitPropertyOnParent1_stopsFocusChange, exitPropertyOnGrandparent_cantStopFocusChange, enterPropertyOnGrandparent_cantStopFocusChange, enterPropertyOnParent2_stopsFocusChange, enterPropertyOnDestination_cantStopFocusFromMoving
#[test]
fn a_move_runs_the_exit_of_the_parent_left_then_the_enter_of_the_parent_entered_and_only_those_can_stop_it()
 {
    let both = ["exit parent1", "enter parent2"];
    // (the block cancelling, the blocks that ran, who ends with the focus)
    let rows: [(Option<(&'static str, bool)>, &[&str], &str); 7] = [
        (None, &both, "child4"),
        (Some(("child1", false)), &both, "child4"),
        (Some(("parent1", false)), &["exit parent1"], "child1"),
        (Some(("grandparent", false)), &both, "child4"),
        (Some(("grandparent", true)), &both, "child4"),
        (Some(("parent2", true)), &both, "child1"),
        (Some(("child4", true)), &both, "child4"),
    ];
    for (cancel, ran, ends_on) in rows {
        let (said, focused) = move_between_cousins(cancel);
        assert_eq!(said, ran, "{cancel:?}");
        assert_eq!(focused, [ends_on], "{cancel:?}");
    }
}

// ---- A target's rectangle ---------------------------------------------------------------------------------

fn rect(left: f32, top: f32, right: f32, bottom: f32) -> Rect {
    Rect::new(left, top, right, bottom)
}

fn inflated(by: f32) -> Modifier {
    properties(move |p| {
        p.focus_rect = rect(
            p.focus_rect.left - by,
            p.focus_rect.top - by,
            p.focus_rect.right + by,
            p.focus_rect.bottom + by,
        );
    })
}

fn rect_of(left: f32, top: f32, right: f32, bottom: f32) -> Modifier {
    properties(move |p| p.focus_rect = rect(left, top, right, bottom))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CustomFocusRectTest.kt, focusRect_boundingBoxByDefault, focusRect_decidedByFocusProperties, focusRect_allAscendantFocusProperties, focusRect_allAscendantFocusProperties_untilAnotherFocusTarget, focusRect_focusProperties_belowFocusTargetNode
#[test]
fn a_targets_rectangle_is_its_bounds_unless_the_properties_of_its_chain_say_otherwise() {
    // (what, the chain before the target, its expected rectangle)
    let rows: [(&str, fn() -> Modifier, Rect); 4] = [
        ("by default", || Modifier.required_size((12.dp(), 12.dp())), rect(0.0, 0.0, 12.0, 12.0)),
        ("set", || rect_of(0.0, 0.0, 70.0, 70.0), rect(0.0, 0.0, 70.0, 70.0)),
        (
            "each block going on from the one after it",
            || inflated(5.0).then(inflated(5.0)).then(rect_of(0.0, 0.0, 12.0, 12.0)),
            rect(-10.0, -10.0, 22.0, 22.0),
        ),
        (
            "up to another target",
            || inflated(5.0).focus_group().then(inflated(5.0)).then(rect_of(0.0, 0.0, 12.0, 12.0)),
            rect(-5.0, -5.0, 17.0, 17.0),
        ),
    ];
    for (what, chain, expected) in rows {
        let target = HeldTarget::new(Focusability::ALWAYS);
        let _scene = scene(move |_| Leaf(chain().then(target.modifier())));
        assert_eq!(target.handle().fetch_focus_rect(), expected, "{what}");
    }

    // A child's aren't its.
    let target = HeldTarget::new(Focusability::ALWAYS);
    let _scene = scene(move |_| {
        Stack!(modifier = rect_of(0.0, 0.0, 12.0, 12.0).then(target.modifier()), {
            Leaf(rect_of(3.0, 3.0, 6.0, 6.0));
        });
    });
    assert_eq!(target.handle().fetch_focus_rect(), rect(0.0, 0.0, 12.0, 12.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CustomFocusRectTest.kt, focusRect_focusPropertiesAttachedLater, focusRect_focusPropertiesDetachedLater, focusRect_canChange_accordingToFocusState
#[test]
fn a_targets_rectangle_follows_properties_coming_going_and_reading_its_state() {
    let target = HeldTarget::new(Focusability::ALWAYS);
    let scene = scene(move |scene| {
        let shown = scene.flag("shown").get();
        let chain = Modifier.required_size((30.dp(), 30.dp())).then(if shown {
            rect_of(0.0, 0.0, 12.0, 12.0)
        } else {
            Modifier
        });
        Leaf(chain.then(target.modifier()));
    });
    let bounds = rect(0.0, 0.0, 30.0, 30.0);
    let custom = rect(0.0, 0.0, 12.0, 12.0);
    assert_eq!(target.handle().fetch_focus_rect(), bounds);
    scene.set("shown", true);
    assert_eq!(target.handle().fetch_focus_rect(), custom, "attached");
    scene.set("shown", false);
    assert_eq!(target.handle().fetch_focus_rect(), bounds, "detached");

    // A block can't read its target's state while the tree asks it (D1280): it reads what the
    // target's listener wrote.
    let target = HeldTarget::new(Focusability::ALWAYS);
    let scene = self::scene(move |scene| {
        let focused = scene.flag("focused");
        let follows = properties(move |p| {
            p.focus_rect =
                if focused.get() { rect(7.0, 7.0, 14.0, 14.0) } else { rect(0.0, 0.0, 70.0, 70.0) };
        });
        let tells = Modifier.on_focus_changed(move |it: FocusState| focused.set(it.is_focused()));
        Leaf(tells.then(follows).then(target.modifier()));
    });
    assert_eq!(target.handle().fetch_focus_rect(), rect(0.0, 0.0, 70.0, 70.0));
    assert!(target.request());
    scene.idle();
    assert_eq!(target.handle().fetch_focus_rect(), rect(7.0, 7.0, 14.0, 14.0));
}
