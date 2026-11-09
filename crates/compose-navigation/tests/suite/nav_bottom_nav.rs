//! Scene decoration with a bottom navigation bar, against Navigation 3's
//! `BottomNavSceneDecoratorTest`: wraps non-overlay scenes whose metadata asks for the bar, and
//! leaves dialogs undecorated.

use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_navigation::*;
use compose_runtime::snapshots::{OwnedStateList, mutable_state_list_from};
use compose_runtime::*;
use compose_test::*;
use compose_ui::{DialogProperties, Modifier};

use crate::nav_support::{FIRST, ListDetailScene, THIRD, remember_list_detail_scene_strategy};

const SECOND_PREFIX: &str = "second";
const BOTTOM_NAV_HOME: &str = "Home";
const BOTTOM_NAV_SETTINGS: &str = "Settings";
const NAV_BAR: &str = "BottomNavScene-Bar";

struct BottomNavSceneDecorator<T: 'static> {
    scene: SceneRef<T>,
}

impl BottomNavSceneDecorator<()> {
    fn show_nav_bar() -> NavMetadata {
        NavMetadata::new().with(NAV_BAR, true)
    }
}

impl<T: Clone + PartialEq + 'static> Scene<T> for BottomNavSceneDecorator<T> {
    fn key(&self) -> ContentKey {
        self.scene.key()
    }

    fn entries(&self) -> &[NavEntry<T>] {
        self.scene.entries()
    }

    fn previous_entries(&self) -> &[NavEntry<T>] {
        self.scene.previous_entries()
    }

    fn content(&self) {
        BottomNavContent(self.scene.clone());
    }

    fn equals(&self, other: &dyn Scene<T>) -> bool {
        other.as_any().downcast_ref::<Self>().is_some_and(|other| *self.scene == *other.scene)
    }
}

#[composable(non_restartable)]
#[allow(non_snake_case)]
fn BottomNavContent<T: Clone + PartialEq + 'static>(scene: SceneRef<T>) {
    Column!(modifier = Modifier.fill_max_size(), move |col: ColumnScope| {
        let scene = scene.clone();
        Row!(modifier = Modifier.weight(col, 0.9), { scene.content() });
        Row!(modifier = Modifier.weight(col, 0.1), {
            for item in [BOTTOM_NAV_HOME, BOTTOM_NAV_SETTINGS] {
                Box!(modifier = Modifier.clickable(|| {}), { BasicText!(item) });
            }
        });
    });
}

#[composable(non_restartable)]
fn remember_bottom_nav_scene_strategy<T: Clone + PartialEq + 'static>()
-> SceneDecoratorStrategyRef<T> {
    remember(|| {
        scene_decorator_strategy(|_: &SceneDecoratorStrategyScope<T>, scene: SceneRef<T>| {
            if scene.metadata().contains_named(NAV_BAR) {
                Rc::new(BottomNavSceneDecorator { scene }) as SceneRef<T>
            } else {
                scene
            }
        })
    })
}

#[composable]
fn TextKey(text: String) {
    BasicText!("{text}");
}

fn plain_entry(key: String) -> NavEntry<String> {
    NavEntry::new(key, |key| TextKey!(key))
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/BottomNavSceneDecoratorTest.kt, testContentShown
#[test]
fn bottom_nav_bar_is_shown_when_scene_requests_it() {
    let owner = mutable_state_list_from([FIRST.to_string()]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let back_stack = stack;
        let decorator = remember_bottom_nav_scene_strategy();
        NavDisplay!(
            back_stack,
            on_back = Some(move || {
                back_stack.remove(back_stack.len() - 1);
            }),
            scene_decorator_strategies = vec![decorator],
            entry_provider = |key: String| {
                if key == FIRST || key.starts_with(SECOND_PREFIX) {
                    plain_entry(key).with_metadata(BottomNavSceneDecorator::show_nav_bar())
                } else if key == THIRD {
                    plain_entry(key)
                } else {
                    panic!("Invalid key passed: {key}")
                }
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_HOME).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_SETTINGS).assert_is_displayed();
}

fn second_shows_bar_display() -> (ComposeTestRule, OwnedStateList<String>) {
    let owner = mutable_state_list_from([FIRST.to_string()]);
    let stack = owner.handle();
    let ui = compose_test(move || {
        let back_stack = stack;
        let decorator = remember_bottom_nav_scene_strategy();
        NavDisplay!(
            back_stack,
            on_back = Some(move || {
                back_stack.remove(back_stack.len() - 1);
            }),
            scene_decorator_strategies = vec![decorator],
            entry_provider = |key: String| {
                if key == FIRST {
                    plain_entry(key)
                } else if key.starts_with(SECOND_PREFIX) {
                    plain_entry(key).with_metadata(BottomNavSceneDecorator::show_nav_bar())
                } else {
                    panic!("Invalid key passed: {key}")
                }
            }
        );
    });
    (ui, owner)
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/BottomNavSceneDecoratorTest.kt, testContentShownOnNavigate
#[test]
fn bottom_nav_bar_appears_when_navigating_to_scene_with_bar_metadata() {
    let (mut ui, stack) = second_shows_bar_display();
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    assert!(!ui.on_node_with_text(BOTTOM_NAV_HOME).is_displayed());
    assert!(!ui.on_node_with_text(BOTTOM_NAV_SETTINGS).is_displayed());

    stack.push(format!("{SECOND_PREFIX}:1"));
    ui.wait_for_idle();

    ui.on_node_with_text(&format!("{SECOND_PREFIX}:1")).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_HOME).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_SETTINGS).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/BottomNavSceneDecoratorTest.kt, testContentRemovedOnPop
#[test]
fn bottom_nav_bar_is_removed_when_popping_back_to_scene_without_bar_metadata() {
    let (mut ui, stack) = second_shows_bar_display();
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    assert!(!ui.on_node_with_text(BOTTOM_NAV_HOME).is_displayed());
    assert!(!ui.on_node_with_text(BOTTOM_NAV_SETTINGS).is_displayed());

    stack.push(format!("{SECOND_PREFIX}:1"));
    ui.wait_for_idle();

    ui.on_node_with_text(&format!("{SECOND_PREFIX}:1")).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_HOME).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_SETTINGS).assert_is_displayed();

    stack.remove(stack.len() - 1);
    ui.wait_for_idle();

    ui.on_node_with_text(FIRST).assert_is_displayed();
    assert!(!ui.on_node_with_text(BOTTOM_NAV_HOME).is_displayed());
    assert!(!ui.on_node_with_text(BOTTOM_NAV_SETTINGS).is_displayed());
}

fn dialog_over_bottom_nav_display() -> (ComposeTestRule, OwnedStateList<String>) {
    let owner = mutable_state_list_from([FIRST.to_string()]);
    let stack = owner.handle();
    let ui = compose_test(move || {
        let back_stack = stack;
        let decorator = remember_bottom_nav_scene_strategy();
        NavDisplay!(
            back_stack,
            on_back = Some(move || {
                back_stack.remove(back_stack.len() - 1);
            }),
            scene_strategies = vec![scene_strategy(DialogSceneStrategy)],
            scene_decorator_strategies = vec![decorator],
            entry_provider = |key: String| {
                if key == FIRST {
                    plain_entry(key).with_metadata(BottomNavSceneDecorator::show_nav_bar())
                } else if key.starts_with(SECOND_PREFIX) {
                    plain_entry(key)
                        .with_metadata(DialogSceneStrategy::dialog(DialogProperties::default()))
                } else {
                    panic!("Invalid key passed: {key}")
                }
            }
        );
    });
    (ui, owner)
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/BottomNavSceneDecoratorTest.kt, testWithThenDialog
#[test]
fn bottom_nav_bar_remains_visible_under_a_dialog() {
    let (mut ui, stack) = dialog_over_bottom_nav_display();
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_HOME).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_SETTINGS).assert_is_displayed();

    stack.push(format!("{SECOND_PREFIX}:1"));
    ui.wait_for_idle();

    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(&format!("{SECOND_PREFIX}:1")).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_HOME).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_SETTINGS).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/BottomNavSceneDecoratorTest.kt, testDialogThenWithBottomNav
#[test]
fn dialog_strategy_with_bottom_nav_decorator_keeps_underlaid_bar() {
    let (mut ui, stack) = dialog_over_bottom_nav_display();
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_HOME).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_SETTINGS).assert_is_displayed();

    stack.push(format!("{SECOND_PREFIX}:1"));
    ui.wait_for_idle();

    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(&format!("{SECOND_PREFIX}:1")).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_HOME).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_SETTINGS).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/BottomNavSceneDecoratorTest.kt, testBottomNavInDialogNotShown
#[test]
fn bottom_nav_metadata_on_a_dialog_entry_does_not_decorate_the_overlay() {
    let owner = mutable_state_list_from([FIRST.to_string()]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let back_stack = stack;
        let decorator = remember_bottom_nav_scene_strategy();
        NavDisplay!(
            back_stack,
            on_back = Some(move || {
                back_stack.remove(back_stack.len() - 1);
            }),
            scene_strategies = vec![scene_strategy(DialogSceneStrategy)],
            scene_decorator_strategies = vec![decorator],
            entry_provider = |key: String| {
                if key == FIRST {
                    plain_entry(key)
                } else if key.starts_with(SECOND_PREFIX) {
                    plain_entry(key).with_metadata(
                        DialogSceneStrategy::dialog(DialogProperties::default())
                            + BottomNavSceneDecorator::show_nav_bar(),
                    )
                } else {
                    panic!("Invalid key passed: {key}")
                }
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();

    stack.push(format!("{SECOND_PREFIX}:1"));
    ui.wait_for_idle();

    // Overlay scenes are not passed through `SceneDecoratorStrategy`.
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(&format!("{SECOND_PREFIX}:1")).assert_is_displayed();
    assert!(!ui.on_node_with_text(BOTTOM_NAV_HOME).is_displayed());
    assert!(!ui.on_node_with_text(BOTTOM_NAV_SETTINGS).is_displayed());
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/BottomNavSceneDecoratorTest.kt, testBottomNavInListDetail
#[test]
fn bottom_nav_decorator_wraps_a_list_detail_scene() {
    let mut ui = compose_test(|| {
        let stack =
            remember(|| mutable_state_list_from([FIRST.to_string(), format!("{SECOND_PREFIX}:0")]));
        let list_detail = remember_list_detail_scene_strategy();
        let bottom_nav = remember_bottom_nav_scene_strategy();
        NavDisplay!(
            stack,
            on_back = Some(move || {
                stack.remove(stack.len() - 1);
            }),
            scene_strategies = vec![list_detail],
            scene_decorator_strategies = vec![bottom_nav],
            entry_provider = |key: String| {
                if key == FIRST {
                    plain_entry(key).with_metadata(ListDetailScene::list_pane())
                } else if key.starts_with(SECOND_PREFIX) {
                    plain_entry(key).with_metadata(
                        ListDetailScene::detail_pane() + BottomNavSceneDecorator::show_nav_bar(),
                    )
                } else if key == THIRD {
                    plain_entry(key)
                } else {
                    panic!("Invalid key passed: {key}")
                }
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(&format!("{SECOND_PREFIX}:0")).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_HOME).assert_is_displayed();
    ui.on_node_with_text(BOTTOM_NAV_SETTINGS).assert_is_displayed();
}
