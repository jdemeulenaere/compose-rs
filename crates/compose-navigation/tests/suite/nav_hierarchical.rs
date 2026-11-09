//! Multi-column hierarchical scenes, against Navigation 3's `HierarchicalSceneTest`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::BasicText;
use compose_foundation::layout::*;
use compose_navigation::*;
use compose_navigation_event::NavigationEvent;
use compose_runtime::snapshots::{OwnedStateList, mutable_state_list_from};
use compose_runtime::*;
use compose_test::*;
use compose_ui::{BackEventCompat, Modifier};

use crate::nav_support::{FIRST, SECOND, Shown, THIRD, text_entry};

struct HierarchicalScene<T: 'static> {
    key: ContentKey,
    nav_entries: Vec<Option<NavEntry<T>>>,
    entries: Vec<NavEntry<T>>,
    previous_entries: Vec<NavEntry<T>>,
}

impl<T: Clone + PartialEq + 'static> Scene<T> for HierarchicalScene<T> {
    fn key(&self) -> ContentKey {
        self.key.clone()
    }

    fn entries(&self) -> &[NavEntry<T>] {
        &self.entries
    }

    fn previous_entries(&self) -> &[NavEntry<T>] {
        &self.previous_entries
    }

    fn content(&self) {
        HierarchicalColumns(self.nav_entries.clone());
    }

    fn equals(&self, other: &dyn Scene<T>) -> bool {
        other.as_any().downcast_ref::<Self>().is_some_and(|other| {
            self.key == other.key
                && self.nav_entries == other.nav_entries
                && self.previous_entries == other.previous_entries
        })
    }
}

#[composable(non_restartable)]
#[allow(non_snake_case)]
fn HierarchicalColumns<T: Clone + 'static>(nav_entries: Vec<Option<NavEntry<T>>>) {
    Row!(move |row: RowScope| {
        for slot in &nav_entries {
            let slot = slot.clone();
            Box!(modifier = Modifier.weight(row, 1.0), move |_| {
                if let Some(entry) = slot.clone() {
                    let content_key = entry.content_key().clone();
                    key(content_key, move || entry.Content());
                }
            });
        }
    });
}

fn hierarchical_strategy<T: Clone + PartialEq + 'static>(columns: usize) -> SceneStrategyRef<T> {
    scene_strategy(move |_: &SceneStrategyScope<T>, entries: &[NavEntry<T>]| {
        let start = entries.len().saturating_sub(columns);
        let included = &entries[start..];
        let nav_entries: Vec<Option<NavEntry<T>>> =
            (0..columns).map(|idx| included.get(idx).cloned()).collect();
        let filtered: Vec<NavEntry<T>> = nav_entries.iter().flatten().cloned().collect();
        let key = filtered.first()?.content_key().clone();
        let previous_entries = if entries.len() > columns {
            entries[..entries.len() - 1].to_vec()
        } else {
            Vec::new()
        };
        let scene: SceneRef<T> =
            Rc::new(HierarchicalScene { key, nav_entries, entries: filtered, previous_entries });
        Some(scene)
    })
}

fn hierarchical_display(
    initial: &[&'static str],
    columns: usize,
) -> (ComposeTestRule, OwnedStateList<&'static str>) {
    let owner = mutable_state_list_from(initial.to_vec());
    let stack = owner.handle();
    let ui = compose_test(move || {
        let back_stack = stack;
        let strategies = remember(|| vec![hierarchical_strategy(columns)]);
        NavDisplay!(
            back_stack,
            on_back = Some(move || {
                back_stack.remove(back_stack.len() - 1);
            }),
            scene_strategies = strategies,
            entry_provider = |key: &'static str| match key {
                FIRST | SECOND | THIRD => text_entry(key),
                other => panic!("Invalid key passed: {other}"),
            }
        );
    });
    (ui, owner)
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/HierarchicalSceneTest.kt, testContentShown
#[test]
fn two_columns_show_first_two_entries() {
    let (mut ui, _stack) = hierarchical_display(&[FIRST, SECOND], 2);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/HierarchicalSceneTest.kt, testContentChanged
#[test]
fn adding_a_third_entry_shifts_the_visible_columns() {
    let (mut ui, stack) = hierarchical_display(&[FIRST, SECOND], 2);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());

    stack.push(THIRD);
    ui.wait_for_idle();

    assert!(!ui.on_node_with_text(FIRST).is_displayed());
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.on_node_with_text(THIRD).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/HierarchicalSceneTest.kt, testOnBack
#[test]
fn popping_restores_the_previous_two_columns() {
    let (mut ui, stack) = hierarchical_display(&[FIRST, SECOND], 2);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());

    stack.push(THIRD);
    ui.wait_for_idle();

    assert!(!ui.on_node_with_text(FIRST).is_displayed());
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.on_node_with_text(THIRD).assert_is_displayed();

    stack.remove(stack.len() - 1);
    ui.wait_for_idle();

    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/HierarchicalSceneTest.kt, testChangingNumberOfColumns
#[test]
fn changing_column_count_and_pressing_back_updates_visible_entries() {
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let columns = mutable_state_of(2usize);
    let cols_read = columns.handle();

    let mut ui = compose_test(move || {
        let count = cols_read.get();
        let strategies = remember_keyed(count, move || vec![hierarchical_strategy(count)]);
        let back_stack = stack;
        NavDisplay!(
            back_stack,
            on_back = Some(move || {
                back_stack.remove(back_stack.len() - 1);
            }),
            scene_strategies = strategies,
            entry_provider = |key: &'static str| match key {
                FIRST | SECOND | THIRD => text_entry(key),
                other => panic!("Invalid key passed: {other}"),
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());

    stack.push(THIRD);
    ui.wait_for_idle();
    assert!(!ui.on_node_with_text(FIRST).is_displayed());
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.on_node_with_text(THIRD).assert_is_displayed();

    columns.set(1);
    ui.wait_for_idle();
    assert!(!ui.on_node_with_text(FIRST).is_displayed());
    assert!(!ui.on_node_with_text(SECOND).is_displayed());
    ui.on_node_with_text(THIRD).assert_is_displayed();

    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    assert!(!ui.on_node_with_text(FIRST).is_displayed());
    ui.on_node_with_text(SECOND).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());

    columns.set(3);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/HierarchicalSceneTest.kt, testStateOfInactiveContentIsRestoredWhenWeGoBackToIt
#[test]
fn state_of_inactive_column_is_restored_when_going_back_to_it() {
    let number_on_screen_1: Rc<RefCell<Option<MutableState<i32>>>> = Rc::default();
    let seen = number_on_screen_1.clone();
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();

    #[composable]
    fn Screen1(seen: Rc<RefCell<Option<MutableState<i32>>>>) {
        let state = remember_saveable(|| mutable_state_of(0));
        *seen.borrow_mut() = Some(state);
    }

    let mut ui = compose_test(move || {
        let back_stack = stack;
        let seen = seen.clone();
        let strategies = remember(|| vec![hierarchical_strategy(2)]);
        NavDisplay!(
            back_stack,
            on_back = Some(move || {
                back_stack.remove(back_stack.len() - 1);
            }),
            scene_strategies = strategies,
            entry_provider = move |key: &'static str| match key {
                FIRST => {
                    let seen = seen.clone();
                    NavEntry::new(FIRST, move |_| Screen1!(seen.clone()))
                }
                SECOND | THIRD => NavEntry::new(key, |_| {}),
                other => panic!("Invalid key passed: {other}"),
            }
        );
    });
    ui.wait_for_idle();

    let n1 = number_on_screen_1.borrow().unwrap();
    assert_eq!(n1.get(), 0, "Initial number should be 0");
    n1.set(n1.get() + 1);
    assert_eq!(n1.get(), 1, "The number should be 1");
    stack.push(THIRD);
    ui.wait_for_idle();

    stack.remove(stack.len() - 1);
    ui.wait_for_idle();

    let restored = number_on_screen_1.borrow().unwrap();
    assert_eq!(restored.get(), 1, "The number should be restored");
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/HierarchicalSceneTest.kt, testStateIsRemovedOnPop
#[test]
fn state_is_removed_on_pop_and_kept_for_remaining_entries() {
    let number_1: Rc<RefCell<Option<MutableState<i32>>>> = Rc::default();
    let number_3: Rc<RefCell<Option<MutableState<i32>>>> = Rc::default();
    let (seen_1, seen_3) = (number_1.clone(), number_3.clone());
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();

    #[composable]
    fn Numbered(label: &'static str, seen: Rc<RefCell<Option<MutableState<i32>>>>) {
        let state = remember_saveable(|| mutable_state_of(0));
        *seen.borrow_mut() = Some(state);
        BasicText!("{label}: {}", state.get());
    }

    let mut ui = compose_test(move || {
        let back_stack = stack;
        let (seen_1, seen_3) = (seen_1.clone(), seen_3.clone());
        let strategies = remember(|| vec![hierarchical_strategy(2)]);
        NavDisplay!(
            back_stack,
            on_back = Some(move || {
                back_stack.remove(back_stack.len() - 1);
            }),
            scene_strategies = strategies,
            entry_provider = move |key: &'static str| match key {
                FIRST => {
                    let seen = seen_1.clone();
                    NavEntry::new(FIRST, move |_| Numbered!("numberOnScreen1", seen.clone()))
                }
                SECOND => NavEntry::new(SECOND, |_| {}),
                THIRD => {
                    let seen = seen_3.clone();
                    NavEntry::new(THIRD, move |_| Numbered!("numberOnScreen3", seen.clone()))
                }
                other => panic!("Invalid key passed: {other}"),
            }
        );
    });
    ui.wait_for_idle();

    let n1 = number_1.borrow().unwrap();
    assert_eq!(n1.get(), 0, "Initial number should be 0");
    n1.set(2);
    ui.wait_for_idle();
    assert_eq!(number_1.borrow().unwrap().get(), 2);
    ui.on_node_with_text("numberOnScreen1: 2").assert_is_displayed();

    stack.push(THIRD);
    ui.wait_for_idle();

    let n3 = number_3.borrow().unwrap();
    assert_eq!(n3.get(), 0, "Initial number should be 0");
    n3.set(4);
    ui.wait_for_idle();
    assert_eq!(number_3.borrow().unwrap().get(), 4);
    ui.on_node_with_text("numberOnScreen3: 4").assert_is_displayed();

    stack.remove(stack.len() - 1);
    ui.wait_for_idle();

    assert_eq!(number_1.borrow().unwrap().get(), 2, "The number should be restored");
    ui.on_node_with_text("numberOnScreen1: 2").assert_is_displayed();

    stack.push(THIRD);
    ui.wait_for_idle();

    assert_eq!(number_3.borrow().unwrap().get(), 0, "Initial number should be 0");
    ui.on_node_with_text("numberOnScreen3: 0").assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/HierarchicalSceneTest.kt, testPredictiveBackShowsCorrectEntries
#[test]
fn predictive_back_shows_all_three_entries_then_completes_to_first_two() {
    let (mut ui, _stack) = hierarchical_display(&[FIRST, SECOND, THIRD], 2);
    ui.wait_for_idle();

    assert!(!ui.on_node_with_text(FIRST).is_displayed());
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.on_node_with_text(THIRD).assert_is_displayed();

    let event = |progress| BackEventCompat {
        touch_x: 0.1,
        touch_y: 0.1,
        progress,
        swipe_edge: NavigationEvent::EDGE_LEFT,
        ..Default::default()
    };
    ui.host().on_back_started(event(0.1));
    ui.host().on_back_progressed(event(0.5));
    ui.wait_for_idle();

    // While predictive back is happening, all 3 entries are shown across the two scenes, with
    // `SECOND` rendered once in the topmost scene.
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.on_node_with_text(THIRD).assert_is_displayed();

    ui.host().on_back_pressed();
    ui.wait_for_idle();

    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    assert!(!ui.on_node_with_text(THIRD).is_displayed());
}

/// When `[FIRST, SECOND]` pushes `THIRD`, `SECOND` moves from column 1 of scene `"first"` to
/// column 0 of scene `"second"`: `SceneSetupNavEntryDecorator`'s movable content keeps its plain
/// `remember` state and avoids disposing its `DisposableEffect`.
#[test]
fn an_entry_shifting_columns_across_scenes_keeps_its_remembered_state() {
    let disposals = Rc::new(Cell::new(0u32));
    let state_2: Rc<RefCell<Option<MutableState<i32>>>> = Rc::default();
    let (disposals_in, state_in) = (disposals.clone(), state_2.clone());
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();

    #[composable]
    fn StatefulSecond(disposals: Rc<Cell<u32>>, held: Rc<RefCell<Option<MutableState<i32>>>>) {
        let count = remember(|| mutable_state_of(0));
        *held.borrow_mut() = Some(count);
        DisposableEffect((), move |scope| {
            scope.on_dispose(move || disposals.set(disposals.get() + 1))
        });
        BasicText!("second: {}", count.get());
    }

    let mut ui = compose_test(move || {
        let strategies = remember(|| vec![hierarchical_strategy(2)]);
        let (disposals, state) = (disposals_in.clone(), state_in.clone());
        NavDisplay!(
            stack,
            scene_strategies = strategies,
            entry_provider = move |key: &'static str| {
                if key == SECOND {
                    let (disposals, state) = (disposals.clone(), state.clone());
                    NavEntry::new(SECOND, move |_| {
                        StatefulSecond!(disposals.clone(), state.clone())
                    })
                } else {
                    NavEntry::new(key, |key| Shown!(key))
                }
            }
        );
    });
    ui.wait_for_idle();
    state_2.borrow().unwrap().set(11);
    ui.wait_for_idle();
    ui.on_node_with_text("second: 11").assert_is_displayed();
    assert_eq!(disposals.get(), 0);

    // Push `THIRD`: `SECOND` moves from column 1 of `HierarchicalScene("first")` to column 0 of
    // `HierarchicalScene("second")`.
    stack.push(THIRD);
    ui.wait_for_idle();
    ui.on_node_with_text("second: 11").assert_is_displayed();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    assert_eq!(disposals.get(), 0, "moved from right column to left column without disposing");

    // Pop `THIRD`: `SECOND` moves back to column 1 of `HierarchicalScene("first")`.
    stack.remove(stack.len() - 1);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text("second: 11").assert_is_displayed();
    assert_eq!(disposals.get(), 0, "moved back to right column without disposing");
}
