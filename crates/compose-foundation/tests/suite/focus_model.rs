//! Foundation's `focusable`, `clickable` and scrollables on the focus model (FO-5), as Compose's
//! `FocusableTest`, `ClickableTest` and `InputModeTest`.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::lazy::*;
use compose_foundation::*;
use compose_runtime::{ReusableContent, SavedState, composable, mutable_state_of, remember};
use compose_test::*;
use compose_ui::platform::UiHost;
use compose_ui::{
    ClickableOptions, Density, Dialog, FocusDirection, FocusManager, FocusRequester, FocusState,
    InputMode, IntSize, Key, KeyEvent, LOCAL_FOCUS_MANAGER, LOCAL_INPUT_MODE_MANAGER,
    SemanticsProperties, UnitExt,
};

/// A 10 dp `tag` of `modifier` with a requester, in `mode`.
fn in_mode(mode: InputMode, clickable: bool) -> (ComposeTestRule, FocusRequester) {
    let requester = FocusRequester::new();
    let mut ui = compose_test(move || {
        let modifier = Modifier.test_tag("tag").size(10.dp()).focus_requester(requester);
        let modifier = if clickable { modifier.clickable(|| {}) } else { modifier.focusable() };
        Box!(modifier = modifier);
    });
    ui.host().set_input_mode(mode);
    ui.wait_for_idle();
    (ui, requester)
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, requestFocus_touchMode
#[test]
fn a_focusable_takes_the_focus_in_touch_mode() {
    let (mut ui, requester) = in_mode(InputMode::Touch, false);
    assert!(requester.request_focus());
    ui.on_node_with_tag("tag").assert_is_focused();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, requestFocus_keyboardMode
#[test]
fn a_focusable_takes_the_focus_in_keyboard_mode() {
    let (mut ui, requester) = in_mode(InputMode::Keyboard, false);
    assert!(requester.request_focus());
    ui.on_node_with_tag("tag").assert_is_focused();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, requestFocus_touchMode
#[test]
fn a_clickable_takes_no_focus_in_touch_mode() {
    let (mut ui, requester) = in_mode(InputMode::Touch, true);
    assert!(!requester.request_focus());
    ui.on_node_with_tag("tag").assert_is_not_focused();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, requestFocus_keyboardMode
#[test]
fn a_clickable_takes_the_focus_in_keyboard_mode() {
    let (mut ui, requester) = in_mode(InputMode::Keyboard, true);
    assert!(requester.request_focus());
    ui.on_node_with_tag("tag").assert_is_focused();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, focusable_focusAcquire
#[test]
fn a_focusable_acquires_the_focus_from_another() {
    let (first, second) = FocusRequester::create_refs();
    let mut ui = compose_test(move || {
        Column!({
            Box!(
                modifier =
                    Modifier.test_tag("first").size(10.dp()).focus_requester(first).focusable()
            );
            Box!(
                modifier =
                    Modifier.test_tag("second").size(10.dp()).focus_requester(second).focusable()
            );
        });
    });
    assert!(first.request_focus());
    ui.on_node_with_tag("first").assert_is_focused();
    ui.on_node_with_tag("second").assert_is_not_focused();
    assert!(second.request_focus());
    ui.on_node_with_tag("first").assert_is_not_focused();
    ui.on_node_with_tag("second").assert_is_focused();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, removingFocusableFromLazyList_clearsFocus
#[test]
fn removing_a_focused_item_clears_the_focus_from_its_list() {
    thread_local! {
        static LIST_HAS_FOCUS: Cell<bool> = const { Cell::new(false) };
    }
    let count = Rc::new(mutable_state_of(20usize));
    let handle = count.handle();
    let state = LazyListState::new(19, 0);
    let given = *state;
    let mut ui = compose_test(move || {
        LazyRow!(
            modifier = Modifier
                .required_size(100.dp())
                .on_focus_changed(|state: FocusState| LIST_HAS_FOCUS.set(state.has_focus())),
            state = given,
            move |scope: LazyListScope| {
                scope.items(handle.get(), |index| {
                    Box!(
                        modifier = Modifier
                            .required_size(10.dp())
                            .test_tag(format!("{index}"))
                            .focusable()
                    );
                });
            }
        );
    });
    ui.on_node_with_tag("19").request_focus();
    ui.wait_for_idle();
    assert!(LIST_HAS_FOCUS.get());
    count.set(11);
    ui.wait_for_idle();
    assert!(!LIST_HAS_FOCUS.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, reusingFocusedItem_itemIsNotFocusedAnymore
#[test]
fn a_focused_item_reused_is_not_focused_any_more() {
    thread_local! {
        static STATE: Cell<FocusState> = const { Cell::new(FocusState::Inactive) };
    }
    let key = Rc::new(mutable_state_of(0));
    let handle = key.handle();
    let requester = FocusRequester::new();
    let mut ui = compose_test(move || {
        ReusableContent(handle.get(), || {
            Box!(
                modifier = Modifier
                    .test_tag("tag")
                    .size(10.dp())
                    .focus_requester(requester)
                    .on_focus_event(|state: FocusState| STATE.set(state))
                    .focusable()
            );
        });
    });
    assert!(requester.request_focus());
    ui.wait_for_idle();
    assert!(STATE.get().is_focused());
    ui.on_node_with_tag("tag").assert_is_focused();
    key.set(1);
    ui.wait_for_idle();
    assert!(!STATE.get().is_focused());
    ui.on_node_with_tag("tag").assert_is_not_focused();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, focusable_requestsBringIntoView_whenFocused
#[test]
fn a_focused_focusable_is_brought_into_view_and_a_plain_target_is_not() {
    let (focusable, target) = FocusRequester::create_refs();
    let scroll = ScrollState::new(0);
    let given = *scroll;
    let mut ui = compose_test(move || {
        Column!(modifier = Modifier.size((50.dp(), 50.dp())).vertical_scroll(given), {
            Box!(modifier = Modifier.size(50.dp()));
            Box!(modifier = Modifier.size(50.dp()).focus_requester(target).focus_target());
            Box!(modifier = Modifier.size(50.dp()).focus_requester(focusable).focusable());
        });
    });
    assert!(target.request_focus());
    ui.wait_for_idle();
    assert_eq!(scroll.value(), 0);
    assert!(focusable.request_focus());
    ui.wait_for_idle();
    assert_eq!(scroll.value(), 100);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/InputModeTest.kt, switchToTouchModeProgrammatically
#[test]
fn the_input_mode_changes_on_request() {
    thread_local! {
        static SEEN: Cell<Option<InputMode>> = const { Cell::new(None) };
    }
    let manager = Rc::new(std::cell::RefCell::new(None));
    let kept = manager.clone();
    let mut ui = compose_test(move || {
        let current = LOCAL_INPUT_MODE_MANAGER.current();
        SEEN.set(Some(current.input_mode()));
        *kept.borrow_mut() = Some(current);
    });
    ui.wait_for_idle();
    assert_eq!(SEEN.get(), Some(InputMode::Keyboard));
    let manager = manager.borrow().clone().expect("composed");
    assert!(manager.request_input_mode(InputMode::Touch));
    ui.wait_for_idle();
    assert_eq!(SEEN.get(), Some(InputMode::Touch), "what reads it composes again");
}

// ---- Lazy lists: targets scrolled away, and restorers over them ---------------------------------------

/// The focus manager of what's composed, kept for the test.
type Manager = Rc<RefCell<Option<FocusManager>>>;

fn manager_of(kept: &Manager) -> FocusManager {
    kept.borrow().clone().expect("composed")
}

/// How a target in a list's first item holds the focus before it's scrolled away.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Holds {
    Nothing,
    Focus,
    Capture,
    Child,
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/DeactivatedFocusNodeTest.kt, deactivatedActiveFocusNodeSendsFocusEvent, deactivatedActiveParentFocusNodeSendsFocusEvent, deactivatedCapturedFocusNodeSendsFocusEvent, deactivatedInactiveFocusNodeDoesNotSendFocusEvent
#[test]
fn an_item_leaving_a_list_tells_its_listener_only_if_it_had_the_focus() {
    for holds in [Holds::Focus, Holds::Child, Holds::Capture, Holds::Nothing] {
        let heard: &'static RefCell<Vec<FocusState>> = Box::leak(Box::default());
        let requester = FocusRequester::new();
        let state = LazyListState::new(0, 0);
        let given = *state;
        let first_item = Box::leak(Box::new(mutable_state_of(0usize))).handle();
        let mut ui = compose_test(move || {
            LazyRow!(
                modifier = Modifier.size(10.dp()),
                state = given,
                move |scope: LazyListScope| {
                    let first_item = first_item.get();
                    for index in [first_item, 100] {
                        scope.item_keyed(index, move || {
                            // Reused, the node's updated element hears it (D1282): any of them.
                            let first = index == 0 || holds != Holds::Nothing;
                            let listening =
                                Modifier.size(10.dp()).on_focus_changed(move |it: FocusState| {
                                    if first {
                                        heard.borrow_mut().push(it);
                                    }
                                });
                            let asked = if index == 0 {
                                Modifier.focus_requester(requester)
                            } else {
                                Modifier
                            };
                            if holds == Holds::Child {
                                Box!(modifier = listening.focus_target(), {
                                    Box!(
                                        modifier = Modifier
                                            .size(5.dp())
                                            .then(asked.clone())
                                            .focus_target()
                                    );
                                });
                            } else {
                                Box!(modifier = listening.then(asked).focus_target());
                            }
                        });
                    }
                }
            );
        });
        ui.wait_for_idle();
        if holds != Holds::Nothing {
            assert!(requester.request_focus());
        }
        if holds == Holds::Capture {
            assert!(requester.capture_focus());
        }
        ui.wait_for_idle();
        heard.borrow_mut().clear();
        // Scrolled away it stays with the focus, pinned (D1282): it's taken out of the list.
        if holds == Holds::Nothing {
            state.request_scroll_to_item(1, 0);
        } else {
            first_item.set(1);
        }
        ui.wait_for_idle();
        let expected: &[FocusState] =
            if holds == Holds::Nothing { &[] } else { &[FocusState::Inactive] };
        assert_eq!(heard.borrow().as_slice(), expected, "{holds:?}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalImplicitEnterTest.kt, moveFocusEnter_emptyLazyList
#[test]
fn a_search_goes_past_an_empty_lazy_list() {
    // (the direction, from, and to: `left` is first among equal distances)
    for (direction, from, to) in [
        (FocusDirection::Left, "right", "left"),
        (FocusDirection::Right, "left", "right"),
        (FocusDirection::Up, "down", "left"),
        (FocusDirection::Down, "up", "left"),
    ] {
        let kept: Manager = Rc::default();
        let keeping = kept.clone();
        let mut ui = compose_test(move || {
            *keeping.borrow_mut() = Some(LOCAL_FOCUS_MANAGER.current());
            let item = |tag: &'static str, x: f32, y: f32| {
                Modifier.offset(x.dp(), y.dp()).size(10.dp()).test_tag(tag).focusable()
            };
            Box!({
                Box!(modifier = item("up", 45.0, 0.0));
                Box!(modifier = item("left", 0.0, 45.0));
                LazyRow!(
                    modifier = Modifier.offset(40.dp(), 40.dp()).size(20.dp()),
                    |_scope: LazyListScope| {}
                );
                Box!(modifier = item("right", 90.0, 45.0));
                Box!(modifier = item("down", 45.0, 90.0));
            });
        });
        ui.on_node_with_tag(from).request_focus();
        ui.wait_for_idle();
        assert!(manager_of(&kept).move_focus(direction), "{direction:?}");
        ui.wait_for_idle();
        ui.on_node_with_tag(to).assert_is_focused();
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, removingActiveItemThatIsBeyondBounds_clearsFocusFromHierarchy
#[test]
fn a_search_to_an_item_laid_out_beyond_the_bounds_keeps_it_only_while_it_is_there() {
    let heard: &'static RefCell<Vec<FocusState>> = Box::leak(Box::default());
    let list_state: &'static Cell<FocusState> =
        Box::leak(Box::new(Cell::new(FocusState::Inactive)));
    let first = FocusRequester::new();
    let kept: Manager = Rc::default();
    let keeping = kept.clone();
    let mut ui = compose_test(move || {
        *keeping.borrow_mut() = Some(LOCAL_FOCUS_MANAGER.current());
        LazyColumn!(
            modifier =
                Modifier.size(10.dp()).on_focus_changed(move |it: FocusState| list_state.set(it)),
            move |scope: LazyListScope| {
                scope.item(move || {
                    Box!(modifier = Modifier.size(10.dp()).focus_requester(first).focus_target());
                });
                scope.item(move || {
                    Box!(
                        modifier = Modifier
                            .size(10.dp())
                            .on_focus_changed(move |it: FocusState| heard.borrow_mut().push(it))
                            .focus_target()
                    );
                });
            }
        );
    });
    assert!(first.request_focus());
    ui.wait_for_idle();
    manager_of(&kept).move_focus(FocusDirection::Next);
    ui.wait_for_idle();
    // Into the second item, for a moment, then the list's scroll puts the first out.
    assert_eq!(heard.borrow().first(), Some(&FocusState::Inactive));
    assert!(heard.borrow().contains(&FocusState::Active));
    assert_eq!(list_state.get().has_focus(), heard.borrow().last().unwrap().has_focus());
}

/// A column of `count` sized items in a restorer, each with a tag and a listener, the first with
/// a requester, and a requester for the column.
fn restoring_column(
    state: LazyListState,
    parent: FocusRequester,
    first: FocusRequester,
    heard: &'static RefCell<Vec<(usize, FocusState)>>,
) -> ComposeTestRule {
    compose_test(move || {
        LazyColumn!(
            modifier = Modifier
                .size(100.dp())
                .focus_requester(parent)
                .focus_restorer(FocusRequester::DEFAULT),
            state = state,
            move |scope: LazyListScope| {
                scope.items(100, move |item| {
                    let asked = if item == 0 { Modifier.focus_requester(first) } else { Modifier };
                    Box!(
                        modifier = Modifier
                            .size(10.dp())
                            .test_tag(format!("item {item}"))
                            .on_focus_changed(move |it: FocusState| heard
                                .borrow_mut()
                                .push((item, it)))
                            .then(asked)
                            .focus_target()
                    );
                });
            }
        );
    })
}

fn last_of(heard: &RefCell<Vec<(usize, FocusState)>>, item: usize) -> Option<FocusState> {
    heard.borrow().iter().rev().find(|(each, _)| *each == item).map(|(_, state)| *state)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRestorerTest.kt, restorationOfFocusableBeyondVisibleBounds
#[test]
fn a_restorer_brings_back_an_item_scrolled_away_and_back() {
    let parent = FocusRequester::new();
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = compose_test(move || {
        LazyColumn!(
            modifier = Modifier
                .size(100.dp())
                .focus_requester(parent)
                .focus_restorer(FocusRequester::DEFAULT),
            state = given,
            move |scope: LazyListScope| {
                scope.items(100, |item| {
                    Box!(
                        modifier =
                            Modifier.size(10.dp()).test_tag(format!("item {item}")).focusable()
                    );
                });
            }
        );
    });
    ui.on_node_with_tag("item 0").request_focus();
    ui.wait_for_idle();
    ui.on_node_with_tag("item 0").assert_is_focused();
    state.request_scroll_to_item(50, 0);
    ui.wait_for_idle();
    ui.on_node_with_tag("item 0").assert_exists();
    ui.host().clear_focus();
    ui.wait_for_idle();
    ui.on_node_with_tag("item 0").assert_does_not_exist();
    state.request_scroll_to_item(0, 0);
    ui.wait_for_idle();
    assert!(parent.request_focus());
    ui.wait_for_idle();
    ui.on_node_with_tag("item 0").assert_is_focused();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRestorerTest.kt, scrollToClearFocus_restorationOfFocusTargetBeyondVisibleBoundsFailed, clearFocus_restorationOfFocusTargetBeyondVisibleBoundsFailed
#[test]
fn a_restorer_whose_item_is_gone_gives_the_focus_to_what_is_shown() {
    for clear_first in [false, true] {
        let heard: &'static RefCell<Vec<(usize, FocusState)>> = Box::leak(Box::default());
        let (parent, first) = (FocusRequester::new(), FocusRequester::new());
        let state = LazyListState::new(0, 0);
        let mut ui = restoring_column(*state, parent, first, heard);
        assert!(first.request_focus());
        ui.wait_for_idle();
        if clear_first {
            ui.host().clear_focus();
            ui.wait_for_idle();
        }
        state.request_scroll_to_item(50, 0);
        ui.wait_for_idle();
        if !clear_first {
            // It has the focus: pinned, where Compose's plain target isn't (D1282). Clearing it
            // lets it go.
            ui.on_node_with_tag("item 0").assert_exists();
            ui.host().clear_focus();
            ui.wait_for_idle();
        }
        ui.on_node_with_tag("item 0").assert_does_not_exist();
        assert!(parent.request_focus());
        ui.wait_for_idle();
        assert_eq!(last_of(heard, 0), Some(FocusState::Inactive), "clear first {clear_first}");
        assert_eq!(last_of(heard, 50), Some(FocusState::Active), "clear first {clear_first}");
    }
}

/// `count` rows of a lazy column, each a lazy row in a restorer, of `count` items 10 wide.
#[composable]
fn NestedRows(rows: usize, columns: usize) {
    LazyColumn!(
        modifier = Modifier.size(100.dp()).focus_restorer(FocusRequester::DEFAULT),
        move |scope: LazyListScope| {
            scope.items(rows, move |row| {
                LazyRow!(
                    modifier = Modifier.focus_restorer(FocusRequester::DEFAULT),
                    move |scope: LazyListScope| {
                        scope.items(columns, move |column| {
                            Box!(
                                modifier = Modifier
                                    .size(10.dp())
                                    .test_tag(format!("{row},{column}"))
                                    .focusable()
                            );
                        });
                    }
                );
            });
        }
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRestorerTest.kt, restoringItemsInNestedLazyList
#[test]
fn nested_lazy_lists_with_restorers_bring_the_focus_back_to_the_child_each_row_left() {
    let kept: Manager = Rc::default();
    let keeping = kept.clone();
    let mut ui = compose_test(move || {
        *keeping.borrow_mut() = Some(LOCAL_FOCUS_MANAGER.current());
        NestedRows(20, 20);
    });
    let manager = manager_of(&kept);
    ui.on_node_with_tag("0,9").request_focus();
    ui.wait_for_idle();
    ui.on_node_with_tag("1,0").request_focus();
    ui.wait_for_idle();
    for _ in 0..18 {
        manager.move_focus(FocusDirection::Down);
        ui.wait_for_idle();
    }
    ui.on_node_with_tag("19,0").assert_is_focused();
    for _ in 0..20 {
        manager.move_focus(FocusDirection::Up);
        ui.wait_for_idle();
    }
    ui.on_node_with_tag("0,9").assert_is_focused();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRestorerTest.kt, disposedLazyRowWithFocusRestorerRestoresFocusCorrectly_firstRow, disposedLazyRowWithFocusRestorerRestoresFocusCorrectly_allRows
#[test]
fn a_row_with_a_restorer_disposed_and_composed_again_still_gives_the_focus_to_what_it_left() {
    let kept: Manager = Rc::default();
    let keeping = kept.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 250), move || {
        *keeping.borrow_mut() = Some(LOCAL_FOCUS_MANAGER.current());
        LazyColumn!(
            vertical_arrangement = Arrangement::spaced_by(12.dp()),
            |scope: LazyListScope| {
                scope.items(50, |row| {
                    LazyRow!(
                        horizontal_arrangement = Arrangement::spaced_by(8.dp()),
                        modifier = Modifier.focus_restorer(FocusRequester::DEFAULT),
                        move |scope: LazyListScope| {
                            scope.items(50, move |column| {
                                Box!(
                                    modifier = Modifier
                                        .size(50.dp())
                                        .test_tag(format!("{row}-{column}"))
                                        .focusable()
                                );
                            });
                        }
                    );
                });
            }
        );
    });
    let manager = manager_of(&kept);
    let press = |ui: &mut ComposeTestRule, direction: FocusDirection, count: usize| {
        for _ in 0..count {
            manager.move_focus(direction);
            ui.wait_for_idle();
        }
    };
    ui.on_node_with_tag("0-0").request_focus();
    ui.wait_for_idle();
    for count in 1..=10 {
        press(&mut ui, FocusDirection::Right, 1);
        ui.on_node_with_tag(&format!("0-{count}")).assert_is_focused();
    }
    press(&mut ui, FocusDirection::Down, 1);
    press(&mut ui, FocusDirection::Left, 10);
    ui.on_node_with_tag("1-0").assert_is_focused();
    // Down until the first row is long gone, and back up to it.
    press(&mut ui, FocusDirection::Down, 25);
    ui.on_node_with_tag("26-0").assert_is_focused();
    press(&mut ui, FocusDirection::Up, 26);
    ui.on_node_with_tag("0-10").assert_is_focused();
}

// ---- Across a recreation -----------------------------------------------------------------------------------

/// A column of restorers over three rows of three, in a host of `saved`: what it finds after
/// `steps`, and its state.
fn rows_in_a_host(
    saved: Option<SavedState>,
    steps: impl FnOnce(&mut ComposeTestRule, &FocusManager),
) -> (SavedState, Vec<String>) {
    let host = UiHost::new(
        IntSize::new(400, 800),
        Density::new(1.0),
        Box::new(compose_ui::platform::FixedTextMeasurer::new()),
    );
    if let Some(saved) = saved {
        host.restore_state(saved);
    }
    let kept: Manager = Rc::default();
    let keeping = kept.clone();
    let mut ui = ComposeTestRule::with_host(host, move || {
        *keeping.borrow_mut() = Some(LOCAL_FOCUS_MANAGER.current());
        Column!({
            for row in 0..3 {
                Row!(modifier = Modifier.focus_restorer(FocusRequester::DEFAULT).focus_group(), {
                    for column in 0..3 {
                        Box!(
                            modifier = Modifier
                                .size(30.dp())
                                .test_tag(format!("r{row}c{column}"))
                                .focusable()
                        );
                    }
                });
            }
        });
    });
    ui.wait_for_idle();
    steps(&mut ui, &manager_of(&kept));
    ui.wait_for_idle();
    let focused = ui
        .on_all_nodes(is_focused())
        .fetch_semantics_nodes()
        .iter()
        .filter_map(|node| {
            node.config.get(&SemanticsProperties::TEST_TAG).map(|tag| tag.to_string())
        })
        .collect();
    (ui.host().save_state(), focused)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRestorerConfigurationChangeTest.kt, restoreFocus_activityRecreation
#[test]
fn a_restorer_brings_the_child_back_across_a_recreated_host() {
    let (saved, _) = rows_in_a_host(None, |ui, _| {
        ui.on_node_with_tag("r1c1").request_focus();
        ui.wait_for_idle();
        ui.on_node_with_tag("r2c0").request_focus();
    });
    // What the window's focus would do in a recreated activity: asking for the row (Compose's
    // flags have the owner do it, on). A search up into it.
    let (_, focused) = rows_in_a_host(Some(saved), |ui, manager| {
        ui.on_node_with_tag("r2c0").request_focus();
        ui.wait_for_idle();
        manager.move_focus(FocusDirection::Up);
    });
    assert_eq!(focused, ["r1c1"]);
}

// ---- Dialogs ----------------------------------------------------------------------------------------------

fn tab(ui: &mut ComposeTestRule, shift: bool) -> bool {
    let mut event = KeyEvent::down(Key::Tab);
    event.modifiers.shift = shift;
    let consumed = ui.host().send_key(event);
    ui.wait_for_idle();
    consumed
}

#[test]
fn shift_tab_in_a_dialog_wraps_within_it_without_stopping_on_its_window() {
    let mut ui = compose_test(|| {
        Dialog!(on_dismiss_request = || {}, {
            Column!({
                Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("a"));
                Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("b"));
            });
        });
    });
    ui.wait_for_idle();
    for (shift, tag) in
        [(false, "a"), (false, "b"), (false, "a"), (true, "b"), (true, "a"), (true, "b")]
    {
        tab(&mut ui, shift);
        ui.on_node_with_tag(tag).assert_is_focused();
    }
}

/// A dialog whose content asks for the focus by `ask`, over a focused target.
fn asks_in_a_dialog(
    ask: fn(FocusRequester),
) -> (ComposeTestRule, compose_runtime::MutableState<bool>) {
    let open = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut ui = compose_test(move || {
        Column!({
            Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("behind"));
            if open.get() {
                Dialog!(on_dismiss_request = move || open.set(false), {
                    let requester = remember(FocusRequester::new);
                    Column!({
                        Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("first"));
                        Box!(
                            modifier = Modifier
                                .size(20.dp())
                                .focus_requester(requester)
                                .focusable()
                                .test_tag("wanted")
                        );
                    });
                    ask(requester);
                });
            }
        });
    });
    ui.on_node_with_tag("behind").request_focus();
    ui.wait_for_idle();
    (ui, open)
}

#[test]
fn a_request_from_a_dialogs_effect_stays_and_closing_gives_the_focus_back() {
    let asks: [fn(FocusRequester); 3] = [
        |requester| {
            compose_runtime::DisposableEffect((), move |scope| {
                requester.request_focus();
                scope.on_dispose(|| {})
            });
        },
        |requester| {
            compose_runtime::SideEffect(move || {
                requester.request_focus();
            });
        },
        |requester| {
            compose_runtime::LaunchedEffect((), async move {
                requester.request_focus();
            });
        },
    ];
    for ask in asks {
        let (mut ui, open) = asks_in_a_dialog(ask);
        open.set(true);
        ui.wait_for_idle();
        ui.on_node_with_tag("wanted").assert_is_focused();
        open.set(false);
        ui.wait_for_idle();
        ui.on_node_with_tag("behind").assert_is_focused();
    }
}

#[test]
fn a_dialogs_button_gives_the_focus_to_a_field_behind_it_as_it_closes_in_either_order() {
    for close_first in [false, true] {
        let open = Box::leak(Box::new(mutable_state_of(true))).handle();
        let field = FocusRequester::new();
        let mut ui = compose_test(move || {
            Column!({
                Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("other"));
                Box!(
                    modifier =
                        Modifier.size(20.dp()).focus_requester(field).focusable().test_tag("field")
                );
                if open.get() {
                    Dialog!(on_dismiss_request = move || open.set(false), {
                        Box!(
                            modifier = Modifier
                                .size(20.dp())
                                .clickable(move || {
                                    if close_first {
                                        open.set(false);
                                    }
                                    field.request_focus();
                                    open.set(false);
                                })
                                .test_tag("ok")
                        );
                    });
                }
            });
        });
        ui.on_node_with_tag("ok").perform_click();
        ui.wait_for_idle();
        ui.on_node_with_tag("field").assert_is_focused();
    }
}

// ---- A scrolling column in a directional search ---------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusGroupTest.kt, twoDimensionalFocusSearch_skipsChildrenIfTheyAreNotInDirectionOfSearch
#[test]
fn a_directional_search_enters_a_scrolled_column_with_overflow_from_either_side() {
    #[derive(Clone, Copy, Debug)]
    enum Side {
        Before,
        After,
    }
    for horizontal in [false, true] {
        for scrolled in [0, 300] {
            for side in [Side::Before, Side::After] {
                let kept: Manager = Rc::default();
                let keeping = kept.clone();
                let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
                    *keeping.borrow_mut() = Some(LOCAL_FOCUS_MANAGER.current());
                    let scroll_state = remember_scroll_state(scrolled);
                    let outside =
                        |tag: &'static str| Modifier.size(50.dp()).focusable().test_tag(tag);
                    let items = move || {
                        for each in 0..20 {
                            Box!(
                                modifier = Modifier
                                    .size(50.dp())
                                    .focusable()
                                    .test_tag(format!("item{each}"))
                            );
                        }
                    };
                    if horizontal {
                        Row!({
                            Box!(modifier = outside("before"));
                            Row!(
                                modifier = Modifier
                                    .size((200.dp(), 50.dp()))
                                    .horizontal_scroll(scroll_state),
                                {
                                    items();
                                }
                            );
                            Box!(modifier = outside("after"));
                        });
                    } else {
                        Column!({
                            Box!(modifier = outside("before"));
                            Column!(
                                modifier = Modifier
                                    .size((50.dp(), 200.dp()))
                                    .vertical_scroll(scroll_state),
                                {
                                    items();
                                }
                            );
                            Box!(modifier = outside("after"));
                        });
                    }
                });
                ui.wait_for_idle();
                let (from, direction) = match (side, horizontal) {
                    (Side::Before, false) => ("before", FocusDirection::Down),
                    (Side::Before, true) => ("before", FocusDirection::Right),
                    (Side::After, false) => ("after", FocusDirection::Up),
                    (Side::After, true) => ("after", FocusDirection::Left),
                };
                ui.on_node_with_tag(from).request_focus();
                ui.wait_for_idle();
                assert!(
                    manager_of(&kept).move_focus(direction),
                    "{side:?} {horizontal} {scrolled}"
                );
                ui.wait_for_idle();
                let focused = ui.on_all_nodes(is_focused()).fetch_semantics_nodes();
                let tag = focused[0]
                    .config
                    .get(&SemanticsProperties::TEST_TAG)
                    .map(|tag| tag.to_string());
                assert!(
                    tag.as_deref().is_some_and(|tag| tag.starts_with("item")),
                    "{side:?} {horizontal} {scrolled}: {tag:?}"
                );
            }
        }
    }
}

// ---- Disabled: not a target ---------------------------------------------------------------------------------

fn disabled_clickable() -> Modifier {
    Modifier.clickable_with(ClickableOptions { enabled: false, ..ClickableOptions::DEFAULT }, || {})
}

#[test]
fn a_listener_above_a_disabled_clickable_hears_its_child() {
    let heard: &'static RefCell<Vec<FocusState>> = Box::leak(Box::default());
    let child = FocusRequester::new();
    let mut ui = compose_test(move || {
        Box!(
            modifier = Modifier
                .on_focus_changed(move |state: FocusState| heard.borrow_mut().push(state))
                .then(disabled_clickable()),
            {
                Box!(modifier = Modifier.size(20.dp()).focus_requester(child).focusable());
            }
        );
    });
    heard.borrow_mut().clear();
    assert!(child.request_focus());
    ui.wait_for_idle();
    assert_eq!(heard.borrow().as_slice(), [FocusState::Active]);
}

#[test]
fn the_properties_above_a_disabled_clickable_are_its_childs() {
    let (first, second) = FocusRequester::create_refs();
    let mut ui = compose_test(move || {
        Column!({
            Box!(
                modifier =
                    Modifier.focus_properties(move |p| p.next = second).then(disabled_clickable()),
                {
                    Box!(modifier = Modifier.size(20.dp()).focus_requester(first).focusable());
                }
            );
            Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("plain"));
            Box!(
                modifier =
                    Modifier.size(20.dp()).focus_requester(second).focusable().test_tag("custom")
            );
        });
    });
    assert!(first.request_focus());
    ui.wait_for_idle();
    tab(&mut ui, false);
    ui.on_node_with_tag("custom").assert_is_focused();
}

#[test]
fn properties_cant_make_a_disabled_clickable_focusable() {
    let requester = FocusRequester::new();
    let mut ui = compose_test(move || {
        Box!(
            modifier = Modifier
                .size(20.dp())
                .focus_requester(requester)
                .focus_properties(|p| p.can_focus = true)
                .then(disabled_clickable())
                .test_tag("disabled")
        );
    });
    assert!(!requester.request_focus());
    ui.wait_for_idle();
    assert_eq!(ui.host().with_tree(|tree| tree.focused()), None);
}

#[test]
fn a_clickable_disabled_with_the_focus_loses_it_and_is_a_target_again_when_enabled() {
    let enabled = Box::leak(Box::new(mutable_state_of(true))).handle();
    let requester = FocusRequester::new();
    let heard: &'static RefCell<Vec<FocusState>> = Box::leak(Box::default());
    let mut ui = compose_test(move || {
        Box!(
            modifier = Modifier
                .size(20.dp())
                .on_focus_changed(move |state: FocusState| heard.borrow_mut().push(state))
                .focus_requester(requester)
                .clickable_with(
                    ClickableOptions { enabled: enabled.get(), ..ClickableOptions::DEFAULT },
                    || {}
                )
                .test_tag("it")
        );
    });
    assert!(requester.request_focus());
    ui.wait_for_idle();
    ui.on_node_with_tag("it").assert_is_focused();
    heard.borrow_mut().clear();
    enabled.set(false);
    ui.wait_for_idle();
    assert_eq!(heard.borrow().last(), Some(&FocusState::Inactive));
    assert_eq!(ui.host().with_tree(|tree| tree.focused()), None);
    assert!(!requester.request_focus());
    enabled.set(true);
    ui.wait_for_idle();
    assert!(requester.request_focus());
    ui.wait_for_idle();
    ui.on_node_with_tag("it").assert_is_focused();
}

// ---- A search waiting for a list to lay out more --------------------------------------------------------------

#[test]
fn a_move_waiting_for_a_lazy_list_answers_yes_and_arrives() {
    let kept: Manager = Rc::default();
    let keeping = kept.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        *keeping.borrow_mut() = Some(LOCAL_FOCUS_MANAGER.current());
        LazyColumn!(modifier = Modifier.size((100.dp(), 50.dp())), |scope: LazyListScope| {
            scope.items(40, |index| {
                let shown = Modifier.size((100.dp(), 30.dp()));
                if index % 15 == 0 {
                    Box!(modifier = shown.focusable().test_tag(format!("i{index}")));
                } else {
                    Box!(modifier = shown);
                }
            });
        });
    });
    ui.on_node_with_tag("i0").request_focus();
    ui.wait_for_idle();
    assert!(manager_of(&kept).move_focus(FocusDirection::Next));
    ui.wait_for_idle();
    ui.on_node_with_tag("i15").assert_is_focused();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRestorerConfigurationChangeTest.kt, restoreFocus_activityRecreation
#[test]
fn a_restorer_brings_the_child_back_across_a_state_restoration_tester() {
    let parent = FocusRequester::new();
    let second = FocusRequester::new();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {});
    let mut tester = StateRestorationTester::new();
    tester.set_content(&mut ui, move || {
        Column!(
            modifier = Modifier
                .focus_requester(parent)
                .focus_restorer(FocusRequester::DEFAULT)
                .focus_group(),
            {
                Box!(modifier = Modifier.size(10.dp()).focusable().test_tag("first"));
                Box!(
                    modifier = Modifier
                        .size(10.dp())
                        .focus_requester(second)
                        .focusable()
                        .test_tag("second")
                );
                Box!(modifier = Modifier.size(10.dp()).focusable().test_tag("third"));
            }
        );
        Box!(modifier = Modifier.size(10.dp()).focusable().test_tag("outside"));
    });
    ui.wait_for_idle();
    assert!(second.request_focus());
    ui.wait_for_idle();
    // It saves as the focus leaves it.
    ui.on_node_with_tag("outside").request_focus();
    ui.wait_for_idle();
    tester.emulate_save_and_restore(&mut ui);
    assert!(parent.request_focus());
    ui.wait_for_idle();
    ui.on_node_with_tag("second").assert_is_focused();
}

#[test]
fn a_text_field_disabled_with_the_focus_loses_it_and_a_listener_above_hears_its_child_after() {
    let enabled = Box::leak(Box::new(mutable_state_of(true))).handle();
    let requester = FocusRequester::new();
    let mut ui = compose_test(move || {
        BasicTextField!(
            String::new(),
            |_| {},
            enabled = enabled.get(),
            single_line = true,
            modifier = Modifier.size((100.dp(), 40.dp())).focus_requester(requester)
        );
    });
    assert!(requester.request_focus());
    ui.wait_for_idle();
    assert!(ui.host().with_tree(|tree| tree.focused()).is_some());
    enabled.set(false);
    ui.wait_for_idle();
    assert_eq!(ui.host().with_tree(|tree| tree.focused()), None);
    assert!(!requester.request_focus());
    enabled.set(true);
    ui.wait_for_idle();
    assert!(requester.request_focus());
}
