//! Scenes, against Navigation 3's `SceneStateTest`, `SceneInfoTest` and `NavigationBackHandlerTest`.
//! `testSceneStrategyThenFirstStrategy` and `testSceneStrategyThenChainedStrategy` are of a deprecated
//! operator, out of scope, and what they show is `nav_display.rs`'s, of a list of strategies.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_navigation::*;
use compose_navigation_event::{
    DirectNavigationEventInput, LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER, NavigationEvent,
    NavigationEventState, NavigationEventTransitionState, TestNavigationEventDispatcherOwner,
    remember_navigation_event_dispatcher_owner,
};
use compose_runtime::snapshots::{OwnedStateList, mutable_state_list_from};
use compose_runtime::*;
use compose_test::*;
use compose_ui::DialogProperties;

use crate::nav_support::*;

fn dialog_entry(key: &'static str) -> NavEntry<&'static str> {
    let entry = text_entry(key);
    if key == FIRST {
        entry
    } else {
        entry.with_metadata(DialogSceneStrategy::dialog(DialogProperties::default()))
    }
}

thread_local! {
    static SCENE_STATE: RefCell<Option<SceneState<&'static str>>> = const { RefCell::new(None) };
}

fn scene_state() -> SceneState<&'static str> {
    SCENE_STATE.with_borrow(|state| state.clone().expect("composed"))
}

fn of_dialogs(keys: &[&'static str]) -> (ComposeTestRule, OwnedStateList<&'static str>) {
    let owner = mutable_state_list_from(keys.to_vec());
    let stack = owner.handle();
    let ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(stack, entry_provider = dialog_entry);
        let state = remember_scene_state!(
            entries,
            vec![scene_strategy(DialogSceneStrategy)],
            on_back = || {}
        );
        SCENE_STATE.with_borrow_mut(|kept| *kept = Some(state));
    });
    (ui, owner)
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/SceneStateTest.kt, testSceneStateChanges
#[test]
fn a_dialog_is_an_overlay_of_the_scene_under_it() {
    let (mut ui, stack) = of_dialogs(&[FIRST]);
    ui.wait_for_idle();
    assert!(scene_state().current_scene().as_overlay().is_none());
    assert!(scene_state().previous_scenes().is_empty());
    assert!(scene_state().overlay_scenes().is_empty());
    stack.push(SECOND);
    ui.wait_for_idle();
    assert!(scene_state().current_scene().as_overlay().is_none());
    assert_eq!(scene_state().previous_scenes().len(), 0);
    assert_eq!(scene_state().overlay_scenes().len(), 1);
    stack.push(THIRD);
    ui.wait_for_idle();
    assert_eq!(scene_state().overlay_scenes().len(), 2, "each dialog");
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/SceneStateTest.kt, testSceneStatePreviousScenes
#[test]
fn what_s_before_a_dialog_s_scene_is_a_previous_scene() {
    let owner = mutable_state_list_from([FIRST, SECOND, THIRD]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            stack,
            entry_provider = |key: &'static str| {
                let entry = text_entry(key);
                if key == THIRD {
                    entry.with_metadata(DialogSceneStrategy::dialog(DialogProperties::default()))
                } else {
                    entry
                }
            }
        );
        let state = remember_scene_state!(
            entries,
            vec![scene_strategy(DialogSceneStrategy)],
            on_back = || {}
        );
        SCENE_STATE.with_borrow_mut(|kept| *kept = Some(state));
    });
    ui.wait_for_idle();
    let key_of = |key| text_entry(key).content_key().clone();
    let state = scene_state();
    let overlay = &state.overlay_scenes()[0];
    assert_eq!(state.overlay_scenes().len(), 1);
    assert_eq!(overlay.entries().len(), 1);
    assert_eq!(*overlay.entries()[0].content_key(), key_of(THIRD));
    assert!(state.current_scene().as_overlay().is_none());
    assert_eq!(state.current_scene().entries().len(), 1);
    assert_eq!(*state.current_scene().entries()[0].content_key(), key_of(SECOND));
    assert_eq!(state.previous_scenes().len(), 1);
    assert_eq!(state.previous_scenes()[0].entries().len(), 1);
    assert_eq!(*state.previous_scenes()[0].entries()[0].content_key(), key_of(FIRST));
}

/// A strategy counting what it's asked, as the test's `CountingSceneStrategy`.
fn counting_strategy(count: &Rc<Cell<u32>>) -> SceneStrategyRef<&'static str> {
    let count = count.clone();
    scene_strategy(
        move |scope: &SceneStrategyScope<&'static str>, entries: &[NavEntry<&'static str>]| {
            count.set(count.get() + 1);
            SinglePaneSceneStrategy.calculate_scene(scope, entries)
        },
    )
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/SceneStateTest.kt, testSceneStateDoesNotRecalculateOnUnrelatedRecomposition
#[test]
fn a_recomposition_doesn_t_make_scenes_again() {
    let count = Rc::new(Cell::new(0));
    let strategy = counting_strategy(&count);
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let tick = mutable_state_of(0);
    let read = tick.handle();
    let states: Rc<RefCell<Vec<SceneState<&'static str>>>> = Rc::default();
    let kept = states.clone();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            stack,
            entry_provider = entry_provider(|scope: &mut EntryProviderScope<&'static str>| {
                scope.entry(FIRST, |key| Shown!(key));
            })
        );
        let _ = read.get();
        let state = remember_scene_state!(entries, vec![strategy.clone()], on_back = || {});
        kept.borrow_mut().push(state);
    });
    ui.wait_for_idle();
    assert_eq!(count.get(), 1);
    for value in 1..=3 {
        tick.set(value);
        ui.wait_for_idle();
    }
    assert_eq!(count.get(), 1);
    let states = states.borrow();
    assert_eq!(states.len(), 4, "it recomposed");
    assert!(states.iter().all(|state| *state == states[0]));
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/SceneStateTest.kt, testSceneStateDoesNotRecalculateOnOnBackChange
#[test]
fn another_on_back_doesn_t_make_scenes_again() {
    let count = Rc::new(Cell::new(0));
    let strategy = counting_strategy(&count);
    let tick = mutable_state_of(0);
    let read = tick.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(vec![FIRST], entry_provider = text_entry);
        let value = read.get();
        let _ = remember_scene_state!(
            entries,
            vec![strategy.clone()],
            on_back = move || {
                let _ = value;
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(count.get(), 1);
    tick.set(1);
    ui.wait_for_idle();
    tick.set(2);
    ui.wait_for_idle();
    assert_eq!(count.get(), 1);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/SceneStateTest.kt, testSceneStateRecalculatesOnEntriesChange
#[test]
fn other_entries_make_scenes_again() {
    let count = Rc::new(Cell::new(0));
    let strategy = counting_strategy(&count);
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(stack, entry_provider = text_entry);
        let _ = remember_scene_state!(entries, vec![strategy.clone()], on_back = || {});
    });
    ui.wait_for_idle();
    assert_eq!(count.get(), 1);
    stack.push(SECOND);
    ui.wait_for_idle();
    assert!(count.get() > 1);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/SceneStateTest.kt, testSceneStateRecalculatesOnStrategyChange
#[test]
fn another_strategy_makes_scenes_again() {
    let (first, second) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (initial, new) = (counting_strategy(&first), counting_strategy(&second));
    let state = mutable_state_of(0);
    let read = state.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(vec![FIRST], entry_provider = text_entry);
        let strategy = if read.get() == 0 { initial.clone() } else { new.clone() };
        let _ = remember_scene_state!(entries, vec![strategy], on_back = || {});
    });
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get()), (1, 0));
    state.set(1);
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get()), (1, 1));
}

/// A scene's decorator wraps what isn't an overlay.
#[test]
fn a_scene_decorator_wraps_what_isn_t_an_overlay() {
    struct Wrapped(SceneRef<&'static str>);
    impl Scene<&'static str> for Wrapped {
        fn key(&self) -> ContentKey {
            self.0.key()
        }
        fn entries(&self) -> &[NavEntry<&'static str>] {
            self.0.entries()
        }
        fn previous_entries(&self) -> &[NavEntry<&'static str>] {
            self.0.previous_entries()
        }
        fn content(&self) {
            self.0.content();
        }
    }
    let decorated = Rc::new(Cell::new(0));
    let counted = decorated.clone();
    let decorator = scene_decorator_strategy(
        move |_: &SceneDecoratorStrategyScope<&'static str>, scene: SceneRef<&'static str>| {
            counted.set(counted.get() + 1);
            Rc::new(Wrapped(scene)) as SceneRef<&'static str>
        },
    );
    let mut ui = compose_test(move || {
        let entries =
            remember_decorated_nav_entries!(vec![FIRST, SECOND], entry_provider = dialog_entry);
        let state = remember_scene_state!(
            entries,
            vec![scene_strategy(DialogSceneStrategy)],
            vec![decorator.clone()],
            on_back = || {}
        );
        SCENE_STATE.with_borrow_mut(|kept| *kept = Some(state));
    });
    ui.wait_for_idle();
    assert_eq!(decorated.get(), 1, "the dialog isn't");
    let state = scene_state();
    assert!(state.current_scene().as_any().downcast_ref::<Wrapped>().is_some());
    assert!(state.overlay_scenes()[0].as_any().downcast_ref::<Wrapped>().is_none());
}

/// A test's scene.
struct FakeScene {
    key: &'static str,
    entries: Vec<NavEntry<&'static str>>,
}

impl Scene<&'static str> for FakeScene {
    fn key(&self) -> ContentKey {
        ContentKey::new(self.key)
    }
    fn entries(&self) -> &[NavEntry<&'static str>] {
        &self.entries
    }
    fn previous_entries(&self) -> &[NavEntry<&'static str>] {
        &[]
    }
    fn content(&self) {}
}

fn fake(key: &'static str) -> SceneRef<&'static str> {
    Rc::new(FakeScene { key, entries: Vec::new() })
}

// Compose: navigation3/navigation3-ui/src/commonTest/kotlin/androidx/navigation3/scene/SceneInfoTest.kt, sceneInfo_defaultsToNullTitleAndUrl
#[test]
fn a_scene_s_info_has_no_title_or_url_by_default() {
    let scene = fake("home");
    let info = SceneInfo::new(scene.clone());
    assert!(*info.scene == *scene);
    assert_eq!(info.title, None);
    assert_eq!(info.url, None);
}

// Compose: navigation3/navigation3-ui/src/commonTest/kotlin/androidx/navigation3/scene/SceneInfoTest.kt, sceneInfo_equalsAndHashCode_includesTitleAndUrl
#[test]
fn a_scene_s_info_compares_its_title_and_url() {
    let info = |title: &str, url: &str| SceneInfo {
        scene: fake("home"),
        title: Some(title.to_string()),
        url: Some(url.to_string()),
    };
    let one = info("Home", "https://example.com/home");
    assert!(one == info("Home", "https://example.com/home"));
    assert!(one != info("Other", "https://example.com/home"));
    assert!(one != info("Home", "https://example.com/other"));
    let scene = format!("{:?}", one.scene);
    assert_eq!(
        format!("{one:?}"),
        format!(
            "SceneInfo(scene={scene}, title=Some(\"Home\"), url=Some(\"https://example.com/home\"))"
        )
    );
}

// Compose: navigation3/navigation3-ui/src/commonTest/kotlin/androidx/navigation3/scene/SceneInfoTest.kt, titleKeyAndUrlKey_storeAndRetrieveComposableLambdas
#[test]
fn title_and_url_are_kept_as_closures() {
    let metadata = metadata(|scope| {
        scope.put(&TitleMetadataKey, Rc::new(|| "Home Title".to_string()));
        scope.put(&UrlMetadataKey, Rc::new(|| "https://example.com/home".to_string()));
    });
    assert_eq!((metadata.get(&TitleMetadataKey).unwrap())(), "Home Title");
    assert_eq!((metadata.get(&UrlMetadataKey).unwrap())(), "https://example.com/home");
}

// Compose: navigation3/navigation3-ui/src/commonTest/kotlin/androidx/navigation3/scene/SceneInfoTest.kt, scene_metadata_retrievesTitleAndUrlFromLastEntryInScene
#[test]
fn a_scene_s_metadata_is_its_last_entry_s() {
    let with = NavEntry::new("A", |_| {}).with_metadata(metadata(|scope| {
        scope.put(&TitleMetadataKey, Rc::new(|| "Title A".to_string()));
        scope.put(&UrlMetadataKey, Rc::new(|| "https://example.com/a".to_string()));
    }));
    let without = NavEntry::new("B", |_| {});
    let last_without = FakeScene { key: "one", entries: vec![with.clone(), without.clone()] };
    assert!(!last_without.metadata().contains(&TitleMetadataKey));
    assert!(!last_without.metadata().contains(&UrlMetadataKey));
    let last_with = FakeScene { key: "two", entries: vec![without, with] };
    assert!(last_with.metadata().contains(&TitleMetadataKey));
    assert!(last_with.metadata().contains(&UrlMetadataKey));
}

#[composable(non_restartable)]
fn HandlingBack(
    state: SceneState<&'static str>,
    on_cancelled: Rc<dyn Fn()>,
    on_completed: Rc<dyn Fn()>,
) {
    let events = remember_scene_navigation_event_state(state.clone());
    SceneNavigationBackHandler!(
        state,
        events,
        on_back_cancelled = move || on_cancelled(),
        on_back_completed = move || on_completed()
    );
}

/// A back handler of `stack`'s scenes, with an input for gestures.
fn handling(
    stack: OwnedStateList<&'static str>,
    strategies: Vec<SceneStrategyRef<&'static str>>,
    on_cancelled: impl Fn() + 'static,
    count: &Rc<Cell<u32>>,
) -> (ComposeTestRule, DirectNavigationEventInput, OwnedStateList<&'static str>) {
    let owner = TestNavigationEventDispatcherOwner::new();
    let input = DirectNavigationEventInput::new();
    let dispatcher = owner.navigation_event_dispatcher();
    dispatcher.add_input(&input);
    let handle = stack.handle();
    let count = count.clone();
    let on_cancelled: Rc<dyn Fn()> = Rc::new(on_cancelled);
    let ui = compose_test(move || {
        let _ = &owner;
        let (strategies, on_cancelled, count) =
            (strategies.clone(), on_cancelled.clone(), count.clone());
        CompositionLocalProvider(
            LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.provides(Some(dispatcher.clone())),
            move || {
                let entries = remember_decorated_nav_entries!(handle, entry_provider = text_entry);
                let state = remember_scene_state!(entries, strategies.clone(), on_back = || {});
                let counted = count.clone();
                HandlingBack(
                    state,
                    on_cancelled.clone(),
                    Rc::new(move || {
                        counted.set(counted.get() + 1);
                        NavList::remove_last(&handle);
                    }),
                );
            },
        );
    });
    (ui, input, stack)
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/NavigationBackHandlerTest.kt, testNavigationBackHandlerSinglePop
#[test]
fn back_pops_what_the_scene_shows() {
    let count = Rc::new(Cell::new(0));
    let single = vec![scene_strategy(SinglePaneSceneStrategy)];
    let (mut ui, input, stack) =
        handling(mutable_state_list_from([FIRST, SECOND]), single, || {}, &count);
    ui.wait_for_idle();
    input.back_completed();
    ui.wait_for_idle();
    assert_eq!(count.get(), 1);
    assert_eq!(stack.iter_cloned().collect::<Vec<_>>(), [FIRST]);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/NavigationBackHandlerTest.kt, testNavigationBackHandlerMultiPop
#[test]
fn back_pops_as_many_as_the_scene_says() {
    /// One entry, going back two.
    struct MultiPop {
        entries: [NavEntry<&'static str>; 1],
        previous: Vec<NavEntry<&'static str>>,
    }
    impl Scene<&'static str> for MultiPop {
        fn key(&self) -> ContentKey {
            self.entries[0].content_key().clone()
        }
        fn entries(&self) -> &[NavEntry<&'static str>] {
            &self.entries
        }
        fn previous_entries(&self) -> &[NavEntry<&'static str>] {
            &self.previous
        }
        fn content(&self) {}
    }
    let strategy = scene_strategy(
        |_: &SceneStrategyScope<&'static str>, entries: &[NavEntry<&'static str>]| {
            let (last, _) = entries.split_last()?;
            let previous = entries[..entries.len().saturating_sub(2)].to_vec();
            Some(Rc::new(MultiPop { entries: [last.clone()], previous }) as SceneRef<&'static str>)
        },
    );
    let count = Rc::new(Cell::new(0));
    let (mut ui, input, stack) = handling(
        mutable_state_list_from([FIRST, SECOND, THIRD]),
        vec![strategy, scene_strategy(SinglePaneSceneStrategy)],
        || {},
        &count,
    );
    ui.wait_for_idle();
    input.back_completed();
    ui.wait_for_idle();
    assert_eq!(count.get(), 2);
    assert_eq!(stack.iter_cloned().collect::<Vec<_>>(), [FIRST]);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/NavigationBackHandlerTest.kt, testNavigationBackHandlerBackCancelled
#[test]
fn a_cancel_is_told_and_pops_nothing() {
    let (count, cancelled) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let told = cancelled.clone();
    let single = vec![scene_strategy(SinglePaneSceneStrategy)];
    let (mut ui, input, stack) = handling(
        mutable_state_list_from([FIRST, SECOND]),
        single,
        move || told.set(told.get() + 1),
        &count,
    );
    ui.wait_for_idle();
    input.back_cancelled();
    ui.wait_for_idle();
    assert_eq!(cancelled.get(), 1);
    assert_eq!(stack.iter_cloned().collect::<Vec<_>>(), [FIRST, SECOND]);
}

/// An entry with a title and URL of its own key.
fn titled_entry(key: &'static str) -> NavEntry<&'static str> {
    NavEntry::new(key, |_| {}).with_metadata(metadata(|scope| {
        scope.put(&TitleMetadataKey, Rc::new(move || format!("{key} title")));
        scope.put(&UrlMetadataKey, Rc::new(move || format!("https://example.com/{key}")));
    }))
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/RememberNavigationEventStateTest.kt, testRememberNavigationEventState
#[test]
fn a_scene_s_navigation_events_follow_its_state() {
    thread_local! {
        static EVENTS: RefCell<Option<NavigationEventState<SceneInfo<&'static str>>>> =
            const { RefCell::new(None) };
    }
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(stack, entry_provider = titled_entry);
        let state = remember_scene_state!(
            entries,
            vec![scene_strategy(SinglePaneSceneStrategy)],
            on_back = || {}
        );
        let events = remember_scene_navigation_event_state(state.clone());
        EVENTS.with_borrow_mut(|kept| *kept = Some(events));
        SCENE_STATE.with_borrow_mut(|kept| *kept = Some(state));
    });
    ui.wait_for_idle();
    let events = || EVENTS.with_borrow(|events| events.clone().unwrap());
    let current = events().current_info();
    assert!(*current.scene == **scene_state().current_scene());
    assert_eq!(current.title.as_deref(), Some("second title"));
    assert_eq!(current.url.as_deref(), Some("https://example.com/second"));
    let back = events().back_info();
    assert_eq!(back.len(), 1);
    assert!(*back[0].scene == *scene_state().previous_scenes()[0]);
    assert_eq!(back[0].title.as_deref(), Some("first title"));
    assert_eq!(back[0].url.as_deref(), Some("https://example.com/first"));

    stack.push(THIRD);
    ui.wait_for_idle();
    let current = events().current_info();
    assert!(*current.scene == **scene_state().current_scene());
    assert_eq!(current.title.as_deref(), Some("third title"));
    assert_eq!(current.url.as_deref(), Some("https://example.com/third"));
    let back = events().back_info();
    assert_eq!(back.len(), 2);
    assert!(*back[0].scene == *scene_state().previous_scenes()[0]);
    assert_eq!(back[0].title.as_deref(), Some("first title"));
    assert!(*back[1].scene == *scene_state().previous_scenes()[1]);
    assert_eq!(back[1].title.as_deref(), Some("second title"));
    assert_eq!(back[1].url.as_deref(), Some("https://example.com/second"));
}

/// The test's scene, with all the entries so far, going back two.
struct AllEntries {
    entries: Vec<NavEntry<&'static str>>,
    previous: Vec<NavEntry<&'static str>>,
}

impl Scene<&'static str> for AllEntries {
    fn key(&self) -> ContentKey {
        self.entries.last().unwrap().content_key().clone()
    }
    fn entries(&self) -> &[NavEntry<&'static str>] {
        &self.entries
    }
    fn previous_entries(&self) -> &[NavEntry<&'static str>] {
        &self.previous
    }
    fn content(&self) {}
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/NavDisplayInfoTest.kt, testPredictiveBackSwipePopulatesSceneInfoCorrectly
#[test]
fn a_gesture_tells_the_scene_it_goes_back_to() {
    let owner = TestNavigationEventDispatcherOwner::new();
    let input = DirectNavigationEventInput::new();
    let dispatcher = owner.navigation_event_dispatcher();
    dispatcher.add_input(&input);
    let keys = ["KEY_1", "KEY_2", "KEY_3", "KEY_4", "KEY_5"];
    let strategy = scene_strategy(
        |_: &SceneStrategyScope<&'static str>, entries: &[NavEntry<&'static str>]| {
            let previous = entries[..entries.len().saturating_sub(2)].to_vec();
            Some(Rc::new(AllEntries { entries: entries.to_vec(), previous })
                as SceneRef<&'static str>)
        },
    );
    let given = dispatcher.clone();
    let mut ui = compose_test(move || {
        let _ = &owner;
        let child = remember_navigation_event_dispatcher_owner!(parent = Some(given.clone()));
        let strategy = strategy.clone();
        CompositionLocalProvider(
            LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.provides(Some(child)),
            move || {
                NavDisplay!(
                    keys.to_vec(),
                    on_back = Some(|| {}),
                    scene_strategies = vec![strategy.clone()],
                    entry_provider = |key: &'static str| NavEntry::new(key, |_| {})
                );
            },
        );
    });
    ui.wait_for_idle();
    input.back_started(NavigationEvent { progress: 0.1, ..Default::default() });
    input.back_progressed(NavigationEvent { progress: 0.5, ..Default::default() });
    ui.wait_for_idle();
    assert!(matches!(
        dispatcher.transition_state(),
        NavigationEventTransitionState::InProgress { .. }
    ));
    let history = dispatcher.history();
    let index = history.current_index() as usize;
    let keys_of = |at: usize| -> Vec<ContentKey> {
        let info = history.merged_history()[at].downcast_ref::<SceneInfo<&'static str>>().unwrap();
        info.scene.entries().iter().map(|entry| entry.content_key().clone()).collect()
    };
    let key_of = |key: &&'static str| NavEntry::new(*key, |_| {}).content_key().clone();
    assert_eq!(keys_of(index), keys.iter().map(key_of).collect::<Vec<_>>());
    assert_eq!(keys_of(index - 1), keys[..3].iter().map(key_of).collect::<Vec<_>>());
}

/// An overlay whose leaving waits: a sheet's hide in Kotlin's, a delay here.
struct SlowOverlay {
    entries: [NavEntry<&'static str>; 1],
    previous: Vec<NavEntry<&'static str>>,
}

impl Scene<&'static str> for SlowOverlay {
    fn key(&self) -> ContentKey {
        self.entries[0].content_key().clone()
    }
    fn entries(&self) -> &[NavEntry<&'static str>] {
        &self.entries
    }
    fn previous_entries(&self) -> &[NavEntry<&'static str>] {
        &self.previous
    }
    fn content(&self) {
        self.entries[0].Content();
    }
    fn as_overlay(&self) -> Option<&dyn OverlayScene<&'static str>> {
        Some(self)
    }
}

impl OverlayScene<&'static str> for SlowOverlay {
    fn overlaid_entries(&self) -> &[NavEntry<&'static str>] {
        &self.previous
    }
    fn on_remove(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
        Box::pin(delay(std::time::Duration::from_millis(300)))
    }
}

fn slow_overlay_strategy() -> SceneStrategyRef<&'static str> {
    scene_strategy(|_: &SceneStrategyScope<&'static str>, entries: &[NavEntry<&'static str>]| {
        let (last, previous) = entries.split_last()?;
        if previous.is_empty() {
            return None;
        }
        let scene = SlowOverlay { entries: [last.clone()], previous: previous.to_vec() };
        Some(Rc::new(scene) as SceneRef<&'static str>)
    })
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/AnimateOverlaySceneTest.kt, testAnimateOut
#[test]
fn an_overlay_stays_while_it_leaves() {
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            scene_strategies = vec![slow_overlay_strategy()],
            entry_provider = text_entry
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.remove(1);
    for _ in 0..8 {
        ui.advance_time_by_frame();
    }
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.advance_time_by(std::time::Duration::from_millis(400));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/AnimateOverlaySceneTest.kt, testNestedBottomSheet
#[test]
fn an_overlay_over_another_doesn_t_show_it_again() {
    let renders = Rc::new(Cell::new(0));
    let counted = renders.clone();
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let counted = counted.clone();
        NavDisplay!(
            stack,
            scene_strategies = vec![slow_overlay_strategy()],
            entry_provider = move |key: &'static str| {
                let counted = counted.clone();
                NavEntry::new(key, move |key| {
                    if key == SECOND {
                        counted.set(counted.get() + 1);
                    }
                    Shown!(key);
                })
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    assert_eq!(renders.get(), 0);
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    assert_eq!(renders.get(), 1);
    stack.push(THIRD);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_exists();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    assert_eq!(renders.get(), 1);
    stack.remove(2);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    ui.advance_time_by(std::time::Duration::from_millis(400));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.on_node_with_text(THIRD).assert_does_not_exist();
    assert_eq!(renders.get(), 1);
}

/// A scene of its fields, as a data class.
#[derive(Clone)]
struct Fields {
    key: &'static str,
    entries: Vec<NavEntry<&'static str>>,
    previous: Vec<NavEntry<&'static str>>,
}

impl Scene<&'static str> for Fields {
    fn key(&self) -> ContentKey {
        ContentKey::new(self.key)
    }
    fn entries(&self) -> &[NavEntry<&'static str>] {
        &self.entries
    }
    fn previous_entries(&self) -> &[NavEntry<&'static str>] {
        &self.previous
    }
    fn content(&self) {}
}

/// The same fields, of another type.
struct OtherFields(Fields);

impl Scene<&'static str> for OtherFields {
    fn key(&self) -> ContentKey {
        self.0.key()
    }
    fn entries(&self) -> &[NavEntry<&'static str>] {
        self.0.entries()
    }
    fn previous_entries(&self) -> &[NavEntry<&'static str>] {
        self.0.previous_entries()
    }
    fn content(&self) {}
}

/// Scenes are equal as data classes: all their fields, and their type.
#[test]
fn scenes_are_equal_by_their_fields_and_type() {
    let (one, two) = (text_entry(FIRST), text_entry(SECOND));
    let scene = Fields { key: "key", entries: vec![one.clone()], previous: vec![two.clone()] };
    let handle = |scene: Fields| Rc::new(scene) as SceneRef<&'static str>;
    assert!(*handle(scene.clone()) == *handle(scene.clone()));
    assert!(*handle(scene.clone()) != *handle(Fields { key: "other", ..scene.clone() }));
    assert!(*handle(scene.clone()) != *handle(Fields { entries: vec![two], ..scene.clone() }));
    assert!(*handle(scene.clone()) != *handle(Fields { previous: Vec::new(), ..scene.clone() }));
    let other = Rc::new(OtherFields(scene.clone())) as SceneRef<&'static str>;
    assert!(*handle(scene) != *other);
}

/// A scene that shows a text and has one key, of a type by the stack's size.
struct Constant<const SIDE: u8>(&'static str);

impl<const SIDE: u8> Scene<&'static str> for Constant<SIDE> {
    fn key(&self) -> ContentKey {
        ContentKey::new("the key")
    }
    fn entries(&self) -> &[NavEntry<&'static str>] {
        &[]
    }
    fn previous_entries(&self) -> &[NavEntry<&'static str>] {
        &[]
    }
    fn content(&self) {
        Shown!(self.0);
    }
}

/// Scenes of other types with one key are other scenes, and animate into one another.
#[test]
fn scenes_of_other_types_animate_even_with_one_key() {
    let strategy = scene_strategy(
        |_: &SceneStrategyScope<&'static str>, entries: &[NavEntry<&'static str>]| {
            let scene: SceneRef<&'static str> = if entries.len() == 1 {
                Rc::new(Constant::<0>("one"))
            } else {
                Rc::new(Constant::<1>("other"))
            };
            Some(scene)
        },
    );
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(stack, scene_strategies = vec![strategy.clone()], entry_provider = text_entry);
    });
    ui.wait_for_idle();
    ui.on_node_with_text("one").assert_exists();
    ui.set_auto_advance(false);
    stack.push(SECOND);
    ui.advance_time_by(std::time::Duration::from_millis(100));
    ui.on_node_with_text("one").assert_exists();
    ui.on_node_with_text("other").assert_exists();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text("one").assert_does_not_exist();
}
