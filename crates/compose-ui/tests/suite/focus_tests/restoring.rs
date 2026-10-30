//! Giving the focus back: `save_focused_child` and `restore_focused_child`, `focus_restorer`.

use compose_runtime::key;

use super::support::*;

/// A parent over two children with keys, the second requested by a name, `parent` on its chain.
fn parent_of_two(scene: Scene, parent: Modifier) {
    Stack!(modifier = scene.req("parent").then(parent).size(10.dp()), {
        key(1, || Leaf(Modifier.size(10.dp()).then(scene.listen("first")).focus_target()));
        key(2, || Leaf(Modifier.size(10.dp()).then(scene.target("second"))));
    });
}

/// The same without keys.
fn parent_of_two_unkeyed(scene: Scene, parent: Modifier) {
    Stack!(modifier = scene.req("parent").then(parent).size(10.dp()), {
        Leaf(Modifier.size(10.dp()).then(scene.listen("first")).focus_target());
        Leaf(Modifier.size(10.dp()).then(scene.target("second")));
    });
}

/// A parent over a child over a grandchild.
fn parent_of_a_chain(scene: Scene, parent: Modifier) {
    Stack!(modifier = scene.req("parent").then(parent).size(10.dp()), {
        Stack!(modifier = Modifier.size(10.dp()).then(scene.listen("child")).focus_target(), {
            Leaf(Modifier.size(10.dp()).then(scene.target("grandchild")));
        });
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RestoreFocusTest.kt, restoresSavedChild, adjacentCallsRestoreTheFocusToTheCorrectChild, doesNotRestoreGrandChild_butFocusesOnChildInstead, noSavedChild_doesNotRestoreChild
#[test]
fn restoring_gives_the_focus_back_to_the_child_saved_not_deeper() {
    let contents: [(fn(Scene, Modifier), &str, &[(&str, FocusState)]); 3] = [
        (parent_of_two, "second", &[("first", Inactive), ("second", Active)]),
        (parent_of_two_unkeyed, "second", &[("first", Inactive), ("second", Active)]),
        (parent_of_a_chain, "grandchild", &[("child", Active), ("grandchild", Inactive)]),
    ];
    for (content, focused, states) in contents {
        let scene = scene(move |scene| content(scene, Modifier.focus_target()));
        assert!(scene.request(focused));
        assert!(scene.requester("parent").save_focused_child());
        scene.clear(false);
        assert!(scene.requester("parent").restore_focused_child());
        for &(name, state) in states {
            assert_eq!(scene.state(name), state, "{name}");
        }
    }

    // Nothing saved, nothing to give back.
    let scene = scene(|scene| parent_of_two(scene, Modifier.focus_target()));
    assert!(scene.request("second"));
    scene.clear(false);
    assert!(!scene.requester("parent").restore_focused_child());
    assert_eq!(scene.states(["first", "second"]), [Inactive, Inactive]);
}

fn restorer(fallback: FocusRequester) -> Modifier {
    Modifier.focus_restorer(fallback).focus_group()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRestorerTest.kt, restoresSavedChild, adjacentCallsRestoresFocusToTheCorrectChild, doesNotRestoreGrandChild_butFocusesOnChildInstead
#[test]
fn a_restorer_gives_a_request_for_its_parent_to_the_child_that_last_had_the_focus() {
    let contents: [(fn(Scene, Modifier), &str, &[(&str, FocusState)]); 3] = [
        (parent_of_two, "second", &[("first", Inactive), ("second", Active)]),
        (parent_of_two_unkeyed, "second", &[("first", Inactive), ("second", Active)]),
        (parent_of_a_chain, "grandchild", &[("child", Active), ("grandchild", Inactive)]),
    ];
    for (content, focused, states) in contents {
        let scene = scene(move |scene| content(scene, restorer(FocusRequester::DEFAULT)));
        assert!(scene.request(focused));
        scene.clear(false);
        assert!(scene.request("parent"));
        for &(name, state) in states {
            assert_eq!(scene.state(name), state, "{name}");
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRestorerTest.kt, restorationFailed_fallbackToOnRestoreFailedDestination, restorationFailed_fallbackUsingFocusRequesterWithoutFocusRequesterModifier, restorationFailed_whenItemDeleted_fallbackIsUsed
#[test]
fn a_restorer_with_nothing_to_restore_uses_its_fallback_or_else_the_first_child() {
    // With the fallback attached.
    let scene = scene(|scene| parent_of_two(scene, restorer(scene.requester("second"))));
    assert!(scene.request("parent"));
    assert_eq!(scene.states(["first", "second"]), [Inactive, Active]);

    // With one not attached to anything.
    let fallback = FocusRequester::new();
    let scene = self::scene(move |scene| parent_of_two(scene, restorer(fallback)));
    assert!(scene.request("parent"));
    assert_eq!(scene.states(["first", "second"]), [Active, Inactive]);

    // With what it would have restored gone.
    let scene = self::scene(|scene| {
        let gone = scene.flag("gone").get();
        Stack!(
            modifier = scene.req("parent").then(restorer(scene.requester("second"))).size(10.dp()),
            {
                key(1, || Leaf(Modifier.size(10.dp()).then(scene.listen("first")).focus_target()));
                key(2, || Leaf(Modifier.size(10.dp()).then(scene.target("second"))));
                if !gone {
                    key(3, || Leaf(Modifier.size(10.dp()).then(scene.target("third"))));
                }
            }
        );
    });
    assert!(scene.request("third"));
    scene.clear(false);
    scene.set("gone", true);
    assert!(scene.request("parent"));
    assert_eq!(scene.states(["first", "second", "third"]), [Inactive, Active, Inactive]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRestorerTest.kt, moveFocus_restoration
#[test]
fn a_search_into_a_restorers_group_comes_back_to_the_child_it_left() {
    let scene = scene(|scene| {
        Column!({
            Leaf(Modifier.size(10.dp()).then(scene.named("above")).focusable());
            Row!(modifier = Modifier.focus_restorer(FocusRequester::DEFAULT).focus_group(), {
                for name in ["a", "b", "c", "d"] {
                    Leaf(Modifier.size(10.dp()).then(scene.named(name)).focusable());
                }
            });
            Leaf(Modifier.size(10.dp()).then(scene.named("below")).focusable());
        });
    });
    assert!(scene.request("c"));
    assert!(scene.move_focus(Down));
    assert_eq!(scene.focused(), ["below"]);
    assert!(scene.move_focus(Up));
    assert_eq!(scene.focused(), ["c"]);
}
