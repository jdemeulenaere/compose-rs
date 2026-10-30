//! The lifecycles of what a display shows, against Navigation 3's `NavDisplayLifecycleTest`.

use std::cell::RefCell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_lifecycle::*;
use compose_navigation::*;
use compose_runtime::snapshots::mutable_state_list_from;
use compose_runtime::*;
use compose_test::*;
use compose_ui::*;

use crate::nav_support::two_pane_strategy;

type Events = Rc<RefCell<Vec<(&'static str, &'static str)>>>;

#[composable]
fn Logging(key: &'static str, events: Events) {
    let (resumed, paused) = (events.clone(), events);
    LifecycleResumeEffect!((), move |scope| {
        resumed.borrow_mut().push((key, "ON_RESUME"));
        let paused = paused.clone();
        scope.on_pause_or_dispose(move |_| paused.borrow_mut().push((key, "ON_PAUSE")))
    });
}

fn logging(events: &Events) -> impl Fn(&'static str) -> NavEntry<&'static str> + Clone + 'static {
    let events = events.clone();
    move |key| {
        let events = events.clone();
        NavEntry::new(key, move |key| Logging!(key, events.clone()))
    }
}

/// As `logging`, each key but "A" and those of `plain` in a dialog.
fn logging_with_metadata(
    events: &Events,
    metadata_of: fn(&'static str) -> NavMetadata,
) -> impl Fn(&'static str) -> NavEntry<&'static str> + Clone + 'static {
    let entry = logging(events);
    move |key| entry(key).with_metadata(metadata_of(key))
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayLifecycleTest.kt, testNavigateForwardFiresLifecycleEventsInOrder
#[test]
fn going_forward_pauses_the_last_then_resumes_the_new() {
    let events = Events::default();
    let provider = logging(&events);
    let owner = mutable_state_list_from(["A"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(stack, on_back = Some(|| {}), entry_provider = provider.clone());
    });
    ui.wait_for_idle();
    stack.push("B");
    ui.wait_for_idle();
    assert_eq!(*events.borrow(), [("A", "ON_RESUME"), ("A", "ON_PAUSE"), ("B", "ON_RESUME")]);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayLifecycleTest.kt, testNavigateBackFiresLifecycleEventsInOrder
#[test]
fn going_back_pauses_the_last_then_resumes_the_one_before() {
    let events = Events::default();
    let provider = logging(&events);
    let owner = mutable_state_list_from(["A", "B"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(stack, on_back = Some(|| {}), entry_provider = provider.clone());
    });
    ui.wait_for_idle();
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(*events.borrow(), [("B", "ON_RESUME"), ("B", "ON_PAUSE"), ("A", "ON_RESUME")]);
}

fn dialog_except_a(key: &'static str) -> NavMetadata {
    if key == "A" {
        NavMetadata::new()
    } else {
        DialogSceneStrategy::dialog(DialogProperties::default())
    }
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayLifecycleTest.kt, testNavigateToDialogKeepsSinglePaneEntryAtStarted
#[test]
fn a_dialog_pauses_what_s_under_it() {
    let events = Events::default();
    let provider = logging_with_metadata(&events, dialog_except_a);
    let owner = mutable_state_list_from(["A"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            on_back = Some(|| {}),
            scene_strategies = vec![scene_strategy(DialogSceneStrategy)],
            entry_provider = provider.clone()
        );
    });
    ui.wait_for_idle();
    stack.push("B");
    ui.wait_for_idle();
    assert_eq!(*events.borrow(), [("A", "ON_RESUME"), ("A", "ON_PAUSE"), ("B", "ON_RESUME")]);
}

fn dialog_b_only(key: &'static str) -> NavMetadata {
    if key == "B" {
        DialogSceneStrategy::dialog(DialogProperties::default())
    } else {
        NavMetadata::new()
    }
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayLifecycleTest.kt, testNavigateFromDialogToSinglePaneKeepsDialogAtStarted
#[test]
fn a_screen_over_a_dialog_pauses_the_dialog() {
    let events = Events::default();
    let provider = logging_with_metadata(&events, dialog_b_only);
    let owner = mutable_state_list_from(["A", "B"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            on_back = Some(|| {}),
            scene_strategies = vec![scene_strategy(DialogSceneStrategy)],
            entry_provider = provider.clone()
        );
    });
    ui.wait_for_idle();
    assert_eq!(*events.borrow(), [("A", "ON_RESUME"), ("A", "ON_PAUSE"), ("B", "ON_RESUME")]);
    stack.push("C");
    ui.wait_for_idle();
    assert_eq!(
        *events.borrow(),
        [
            ("A", "ON_RESUME"),
            ("A", "ON_PAUSE"),
            ("B", "ON_RESUME"),
            ("B", "ON_PAUSE"),
            ("C", "ON_RESUME"),
        ]
    );
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayLifecycleTest.kt, testNavigateToSecondDialogKeepsFirstDialogAtStarted
#[test]
fn a_second_dialog_pauses_the_first() {
    let events = Events::default();
    let provider = logging_with_metadata(&events, dialog_except_a);
    let owner = mutable_state_list_from(["A", "B"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            on_back = Some(|| {}),
            scene_strategies = vec![scene_strategy(DialogSceneStrategy)],
            entry_provider = provider.clone()
        );
    });
    ui.wait_for_idle();
    assert_eq!(*events.borrow(), [("A", "ON_RESUME"), ("A", "ON_PAUSE"), ("B", "ON_RESUME")]);
    stack.push("C");
    ui.wait_for_idle();
    assert_eq!(
        *events.borrow(),
        [
            ("A", "ON_RESUME"),
            ("A", "ON_PAUSE"),
            ("B", "ON_RESUME"),
            ("B", "ON_PAUSE"),
            ("C", "ON_RESUME"),
        ]
    );
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayLifecycleTest.kt, testTwoPaneForwardFiresLifecycleEventsInOrder
#[test]
fn two_panes_going_forward_resume_both() {
    let events = Events::default();
    let provider = logging(&events);
    let owner = mutable_state_list_from(["A"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            scene_strategies = vec![two_pane_strategy()],
            entry_provider = provider.clone()
        );
    });
    ui.wait_for_idle();
    stack.push("B");
    ui.wait_for_idle();
    assert_eq!(
        *events.borrow(),
        [("A", "ON_RESUME"), ("A", "ON_PAUSE"), ("A", "ON_RESUME"), ("B", "ON_RESUME")]
    );
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayLifecycleTest.kt, testTwoPaneBackFiresLifecycleEventsInOrder
#[test]
fn two_panes_going_back_resume_the_one_left() {
    let events = Events::default();
    let provider = logging(&events);
    let owner = mutable_state_list_from(["A", "B"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            scene_strategies = vec![two_pane_strategy()],
            entry_provider = provider.clone()
        );
    });
    ui.wait_for_idle();
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(
        *events.borrow(),
        [
            ("A", "ON_RESUME"),
            ("B", "ON_RESUME"),
            ("A", "ON_PAUSE"),
            ("B", "ON_PAUSE"),
            ("A", "ON_RESUME"),
        ]
    );
}

struct OverlayKey;

impl NavMetadataKey for OverlayKey {
    type Value = bool;
}

/// The test's `MyCustomOverlayScene`.
struct CustomOverlay {
    key: ContentKey,
    entries: [NavEntry<&'static str>; 1],
    previous_entries: Vec<NavEntry<&'static str>>,
}

impl Scene<&'static str> for CustomOverlay {
    fn key(&self) -> ContentKey {
        self.key.clone()
    }

    fn entries(&self) -> &[NavEntry<&'static str>] {
        &self.entries
    }

    fn previous_entries(&self) -> &[NavEntry<&'static str>] {
        &self.previous_entries
    }

    fn content(&self) {
        OverlayContent(self.entries[0].clone());
    }

    fn as_overlay(&self) -> Option<&dyn OverlayScene<&'static str>> {
        Some(self)
    }
}

impl OverlayScene<&'static str> for CustomOverlay {
    fn overlaid_entries(&self) -> &[NavEntry<&'static str>] {
        &self.previous_entries
    }
}

#[composable]
fn OverlayContent(entry: NavEntry<&'static str>) {
    Box!(modifier = Modifier.size(50.dp()), { entry.Content() });
}

fn overlay_strategy() -> SceneStrategyRef<&'static str> {
    scene_strategy(|_: &SceneStrategyScope<&'static str>, entries: &[NavEntry<&'static str>]| {
        let (last, previous) = entries.split_last()?;
        last.metadata().get(&OverlayKey)?;
        let scene: SceneRef<&'static str> = Rc::new(CustomOverlay {
            key: last.content_key().clone(),
            entries: [last.clone()],
            previous_entries: previous.to_vec(),
        });
        Some(scene)
    })
}

fn overlay_except_a(key: &'static str) -> NavMetadata {
    if key == "A" { NavMetadata::new() } else { metadata(|scope| scope.put(&OverlayKey, true)) }
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayLifecycleTest.kt, testNavigateToSecondOverlayKeepsFirstOverlayAtStarted
#[test]
fn a_second_overlay_pauses_the_first() {
    let events = Events::default();
    let provider = logging_with_metadata(&events, overlay_except_a);
    let owner = mutable_state_list_from(["A", "B"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            on_back = Some(|| {}),
            scene_strategies = vec![overlay_strategy()],
            entry_provider = provider.clone()
        );
    });
    ui.wait_for_idle();
    assert_eq!(*events.borrow(), [("A", "ON_RESUME"), ("A", "ON_PAUSE"), ("B", "ON_RESUME")]);
    stack.push("C");
    ui.wait_for_idle();
    assert_eq!(
        *events.borrow(),
        [
            ("A", "ON_RESUME"),
            ("A", "ON_PAUSE"),
            ("B", "ON_RESUME"),
            ("B", "ON_PAUSE"),
            ("C", "ON_RESUME"),
        ]
    );
}

/// An entry leaving, off the back stack, is created at most, as `BackStackAwareLifecycle…`; the one
/// staying isn't.
#[test]
fn an_entry_leaving_is_capped_at_created() {
    thread_local! {
        static STATES: RefCell<Vec<(&'static str, LifecycleState)>> =
            const { RefCell::new(Vec::new()) };
    }
    #[composable]
    fn NotingState(key: &'static str) {
        let lifecycle = LOCAL_LIFECYCLE_OWNER.current();
        let state = lifecycle.as_state().get();
        STATES.with_borrow_mut(|states| states.push((key, state)));
    }
    let owner = mutable_state_list_from(["A", "B"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            on_back = Some(|| {}),
            entry_provider = |key: &'static str| NavEntry::new(key, |key| NotingState!(key))
        );
    });
    ui.wait_for_idle();
    STATES.with_borrow_mut(Vec::clear);
    stack.remove(1);
    ui.wait_for_idle();
    STATES.with_borrow(|states| {
        assert!(states.iter().any(|(key, state)| *key == "B" && *state == LifecycleState::Created));
        assert_eq!(states.last(), Some(&("A", LifecycleState::Resumed)));
    });
}
