//! `ListDetailScene`, against Navigation 3's `ListDetailSceneTest`: a 40/60 list-detail scene
//! whose detail pane animates in place while the list entry stays or moves to and from a single
//! pane with its state intact.

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::BasicText;
use compose_navigation::*;
use compose_runtime::snapshots::{OwnedStateList, mutable_state_list_from};
use compose_runtime::*;
use compose_test::*;

use crate::nav_support::{
    FIRST, ListDetailScene, THIRD, list_detail_strategy, remember_list_detail_scene_strategy,
};

const SECOND_PREFIX: &str = "second";

fn second(index: usize) -> String {
    format!("{SECOND_PREFIX}:{index}")
}

#[composable]
fn TextKey(text: String) {
    BasicText!("{text}");
}

fn list_detail_entry(key: String) -> NavEntry<String> {
    if key == FIRST {
        NavEntry::new(key, |key| TextKey!(key)).with_metadata(ListDetailScene::list_pane())
    } else if key.starts_with(SECOND_PREFIX) {
        NavEntry::new(key, |key| TextKey!(key)).with_metadata(ListDetailScene::detail_pane())
    } else if key == THIRD {
        NavEntry::new(key, |key| TextKey!(key))
    } else {
        panic!("Invalid key passed: {key}")
    }
}

fn list_detail_display(initial: &[&str]) -> (ComposeTestRule, OwnedStateList<String>) {
    let owner = mutable_state_list_from(initial.iter().map(|&k| k.to_string()).collect::<Vec<_>>());
    let stack = owner.handle();
    let ui = compose_test(move || {
        let back_stack = stack;
        let strategy = remember_list_detail_scene_strategy();
        NavDisplay!(
            back_stack,
            on_back = Some(move || {
                back_stack.remove(back_stack.len() - 1);
            }),
            scene_strategies = vec![strategy],
            entry_provider = list_detail_entry
        );
    });
    (ui, owner)
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/ListDetailSceneTest.kt, testContentShown
#[test]
fn list_and_detail_panes_are_shown_side_by_side() {
    let (mut ui, _stack) = list_detail_display(&[FIRST, &second(0)]);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(&second(0)).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/ListDetailSceneTest.kt, testNavigateToSecondDetail
#[test]
fn navigating_to_a_second_detail_replaces_the_first_while_keeping_the_list() {
    let (mut ui, stack) = list_detail_display(&[FIRST, &second(0)]);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(&second(0)).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());

    stack.push(second(1));
    ui.wait_for_idle();

    ui.on_node_with_text(FIRST).assert_is_displayed();
    assert!(!ui.on_node_with_text(&second(0)).is_displayed());
    ui.on_node_with_text(&second(1)).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/ListDetailSceneTest.kt, testReplaceDetail
#[test]
fn replacing_the_detail_entry_updates_the_detail_pane() {
    let (mut ui, stack) = list_detail_display(&[FIRST, &second(0)]);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(&second(0)).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());

    stack.retain(|it| !it.starts_with(SECOND_PREFIX));
    stack.push(second(1));
    ui.wait_for_idle();

    ui.on_node_with_text(FIRST).assert_is_displayed();
    assert!(!ui.on_node_with_text(&second(0)).is_displayed());
    ui.on_node_with_text(&second(1)).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/ListDetailSceneTest.kt, testNavigateToFullScreen
#[test]
fn navigating_to_a_full_screen_entry_replaces_list_and_detail() {
    let (mut ui, stack) = list_detail_display(&[FIRST, &second(0)]);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(&second(0)).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());

    stack.push(THIRD.to_string());
    ui.wait_for_idle();

    assert!(!ui.on_node_with_text(FIRST).is_displayed());
    assert!(!ui.on_node_with_text(&second(0)).is_displayed());
    ui.on_node_with_text(THIRD).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/ListDetailSceneTest.kt, testOnBackFromFullScreen
#[test]
fn popping_from_full_screen_restores_list_and_detail() {
    let (mut ui, stack) = list_detail_display(&[FIRST, &second(0)]);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(&second(0)).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());

    stack.push(THIRD.to_string());
    ui.wait_for_idle();

    assert!(!ui.on_node_with_text(FIRST).is_displayed());
    assert!(!ui.on_node_with_text(&second(0)).is_displayed());
    ui.on_node_with_text(THIRD).assert_is_displayed();

    stack.remove(stack.len() - 1);
    ui.wait_for_idle();

    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(&second(0)).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());
}

/// Moving an entry from a single-pane scene into a list-detail scene and back preserves its plain
/// `remember` state and keeps its `DisposableEffect` alive without disposing (`SceneSetupNavEntryDecorator`).
#[test]
fn an_entry_moving_into_and_out_of_a_list_detail_scene_keeps_its_remembered_state() {
    let disposals = Rc::new(Cell::new(0u32));
    let held_state: Rc<std::cell::RefCell<Option<MutableState<i32>>>> = Rc::default();
    let owner = mutable_state_list_from([FIRST.to_string()]);
    let stack = owner.handle();
    let (disposals_in, state_in) = (disposals.clone(), held_state.clone());
    let wide = mutable_state_of(true);
    let wide_read = wide.handle();

    #[composable]
    fn StatefulList(
        disposals: Rc<Cell<u32>>,
        held: Rc<std::cell::RefCell<Option<MutableState<i32>>>>,
    ) {
        let count = remember(|| mutable_state_of(0));
        *held.borrow_mut() = Some(count);
        DisposableEffect((), move |scope| {
            scope.on_dispose(move || disposals.set(disposals.get() + 1))
        });
        BasicText!("list: {}", count.get());
    }

    let mut ui = compose_test(move || {
        let is_wide = wide_read.get();
        let strategy = remember_keyed(is_wide, move || list_detail_strategy(is_wide));
        let (disposals, state) = (disposals_in.clone(), state_in.clone());
        NavDisplay!(
            stack,
            scene_strategies = vec![strategy],
            entry_provider = move |key: String| {
                if key == FIRST {
                    let (disposals, state) = (disposals.clone(), state.clone());
                    NavEntry::new(key, move |_| StatefulList!(disposals.clone(), state.clone()))
                        .with_metadata(ListDetailScene::list_pane())
                } else {
                    NavEntry::new(key, |key| TextKey!(key))
                        .with_metadata(ListDetailScene::detail_pane())
                }
            }
        );
    });
    ui.wait_for_idle();
    held_state.borrow().unwrap().set(7);
    ui.wait_for_idle();
    ui.on_node_with_text("list: 7").assert_is_displayed();
    assert_eq!(disposals.get(), 0);

    // Push a detail entry: `FIRST` moves from `SinglePaneScene` into `ListDetailScene`'s left column.
    stack.push(second(0));
    ui.wait_for_idle();
    ui.on_node_with_text("list: 7").assert_is_displayed();
    ui.on_node_with_text(&second(0)).assert_is_displayed();
    assert_eq!(disposals.get(), 0, "moved into list-detail without disposing");

    // Pop the detail entry: `FIRST` moves back from `ListDetailScene` into `SinglePaneScene`.
    stack.remove(stack.len() - 1);
    ui.wait_for_idle();
    ui.on_node_with_text("list: 7").assert_is_displayed();
    assert_eq!(disposals.get(), 0, "moved back to single-pane without disposing");
}
