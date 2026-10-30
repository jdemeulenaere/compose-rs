//! What's on top during a display's transitions, and how scenes' transitions go, against Navigation
//! 3's `NavDisplayScreenshotTest` through clicks and bounds rather than screenshots.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_navigation::ContentKey;
use compose_navigation::*;
use compose_runtime::snapshots::{OwnedStateList, mutable_state_list_from};
use compose_runtime::*;
use compose_test::*;
use compose_ui::*;

use crate::nav_support::*;

type Scope = AnimatedContentTransitionScope<SceneRef<&'static str>>;

const DURATION: u32 = 500;

fn half() -> Duration {
    Duration::from_millis(u64::from(DURATION) / 2)
}

fn third() -> Duration {
    Duration::from_millis(u64::from(DURATION) / 3)
}

thread_local! {
    static CLICKED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

#[composable]
fn Clicking(key: &'static str) {
    Box!(
        modifier = Modifier
            .fill_max_size()
            .clickable(move || CLICKED.with_borrow_mut(|all| all.push(key))),
        { BasicText!(key) }
    );
}

fn clicking_entry(key: &'static str) -> NavEntry<&'static str> {
    NavEntry::new(key, |key| Clicking!(key))
}

/// What's hit at `at`: what's on top there.
fn on_top_at(ui: &mut ComposeTestRule, at: Offset) -> &'static str {
    CLICKED.with_borrow_mut(Vec::clear);
    ui.on_root().perform_touch_input(|touch| touch.click_at(at));
    CLICKED.with_borrow(|all| *all.last().expect("something took the click"))
}

/// What's hit at the middle, one pixel to the left, what's always under both scenes sliding left to
/// right: the one entering ends at the middle when it starts.
fn on_top(ui: &mut ComposeTestRule) -> &'static str {
    let bounds = ui.on_root().fetch_semantics_snapshot().bounds;
    on_top_at(ui, Offset::new(bounds.width() / 2.0 - 1.0, bounds.height() / 2.0))
}

/// The same when both slide right to left.
fn on_top_of_pop(ui: &mut ComposeTestRule) -> &'static str {
    let bounds = ui.on_root().fetch_semantics_snapshot().bounds;
    on_top_at(ui, Offset::new(bounds.width() / 2.0 + 1.0, bounds.height() / 2.0))
}

/// Both slide left to right: what's entering is under what's leaving, or over it, by their z-index.
fn left_to_right(_: Scope) -> ContentTransform {
    slide_in_horizontally_with(tween(DURATION), |width| -width / 2)
        .together_with(slide_out_horizontally_with(tween(DURATION), |width| width / 2))
}

/// Both slide right to left.
fn right_to_left(_: Scope) -> ContentTransform {
    slide_in_horizontally_with(tween(DURATION), |width| width / 2)
        .together_with(slide_out_horizontally_with(tween(DURATION), |width| -width / 2))
}

fn sliding(keys: &[&'static str]) -> (ComposeTestRule, OwnedStateList<&'static str>) {
    let owner = mutable_state_list_from(keys.to_vec());
    let stack = owner.handle();
    let ui = compose_test(move || {
        NavDisplay!(
            stack,
            transition_spec = left_to_right,
            pop_transition_spec = right_to_left,
            entry_provider = clicking_entry
        );
    });
    (ui, owner)
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testNavigateZIndex
#[test]
fn what_enters_is_on_top() {
    let (mut ui, stack) = sliding(&[FIRST]);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.push(SECOND);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(on_top(&mut ui), SECOND);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testNavigateInterruptedZIndex
#[test]
fn what_enters_last_is_on_top_of_what_was_entering() {
    let (mut ui, stack) = sliding(&[FIRST]);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.push(SECOND);
    ui.advance_time_by(third());
    assert_eq!(on_top(&mut ui), SECOND);
    stack.push(THIRD);
    ui.advance_time_by(third());
    assert_eq!(on_top(&mut ui), THIRD);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(on_top(&mut ui), THIRD);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testPopInterruptedZIndex
#[test]
fn what_s_popped_stays_on_top_through_another_pop() {
    let (mut ui, stack) = sliding(&[FIRST, SECOND, THIRD]);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.remove(2);
    ui.advance_time_by(third());
    assert_eq!(on_top_of_pop(&mut ui), THIRD);
    stack.remove(1);
    ui.advance_time_by(third());
    assert_eq!(on_top_of_pop(&mut ui), THIRD);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(on_top_of_pop(&mut ui), FIRST);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testPopZIndex
#[test]
fn what_s_popped_is_on_top() {
    let (mut ui, stack) = sliding(&[FIRST, SECOND]);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.remove(1);
    ui.advance_time_by(half());
    assert_eq!(on_top_of_pop(&mut ui), SECOND);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testTwoPanePopZIndex
#[test]
fn a_scene_of_two_panes_popped_is_on_top() {
    let owner = mutable_state_list_from([FIRST, SECOND, THIRD]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            scene_strategies = vec![two_pane_strategy()],
            pop_transition_spec = |_: Scope| {
                slide_in_vertically_with(tween(DURATION), |height| -height / 2)
                    .together_with(slide_out_vertically_with(tween(DURATION), |height| height / 2))
            },
            entry_provider = clicking_entry
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    let bounds = ui.on_root().fetch_semantics_snapshot().bounds;
    let right = Offset::new(bounds.width() * 3.0 / 4.0, bounds.height() / 2.0);
    ui.set_auto_advance(false);
    stack.remove(2);
    ui.advance_time_by(half());
    assert_eq!(on_top_at(&mut ui, right), THIRD);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(on_top_at(&mut ui, right), SECOND);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testPopNavigateDuplicateZIndex
#[test]
fn a_pop_of_all_and_an_add_of_one_again_is_a_navigation() {
    let (mut ui, stack) = sliding(&[FIRST, SECOND, THIRD]);
    ui.wait_for_idle();
    stack.push(FOURTH);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.remove(3);
    stack.remove(2);
    stack.remove(1);
    stack.push(THIRD);
    ui.advance_time_by(half());
    assert_eq!(on_top(&mut ui), THIRD);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testPopNavigateZIndex
#[test]
fn a_pop_and_an_add_at_once_put_what_s_added_on_top() {
    let (mut ui, stack) = sliding(&[FIRST, SECOND, THIRD]);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.remove(2);
    stack.remove(1);
    stack.push(FOURTH);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.on_node_with_text(FOURTH).assert_is_displayed();
    assert_eq!(on_top(&mut ui), FOURTH);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testNavigateDuplicateZIndex
#[test]
fn a_key_added_again_goes_on_top() {
    let (mut ui, stack) = sliding(&[FIRST]);
    ui.wait_for_idle();
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.push(FIRST);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(on_top(&mut ui), FIRST);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testPopDuplicateZIndex
#[test]
fn a_key_added_again_and_popped_stays_on_top_as_it_leaves() {
    let (mut ui, stack) = sliding(&[FIRST]);
    ui.wait_for_idle();
    for key in [SECOND, THIRD, SECOND] {
        stack.push(key);
        ui.wait_for_idle();
    }
    ui.set_auto_advance(false);
    stack.remove(3);
    ui.advance_time_by(half());
    assert_eq!(on_top_of_pop(&mut ui), SECOND);
}

/// The test's `TestAnimatedTwoPaneScene`: its own transitions, over or under its last entry's.
struct AnimatedTwoPanes {
    entries: [NavEntry<&'static str>; 2],
    previous: Vec<NavEntry<&'static str>>,
    override_entry_animations: bool,
}

const SCENE_DURATION: u32 = 200;

impl Scene<&'static str> for AnimatedTwoPanes {
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
        TwoPanes(self.entries[0].clone(), self.entries[1].clone());
    }

    fn metadata(&self) -> NavMetadata {
        let slide = |_: Scope| {
            Some(ContentTransform::new(
                slide_in_horizontally_with(tween(SCENE_DURATION), |width| width / 2),
                slide_out_horizontally_with(tween(SCENE_DURATION), |width| -width / 2),
            ))
        };
        let own = NavDisplay::transition_spec(slide) + NavDisplay::pop_transition_spec(slide);
        let entry = self.entries[1].metadata().clone();
        if self.override_entry_animations { entry + own } else { own + entry }
    }
}

#[composable]
fn TwoPanes(left: NavEntry<&'static str>, right: NavEntry<&'static str>) {
    Row!(modifier = Modifier.fill_max_width(), |scope: RowScope| {
        Column!(modifier = Modifier.weight(scope, 1.0), { left.Content() });
        Column!(modifier = Modifier.weight(scope, 1.0), { right.Content() });
    });
}

fn animated_two_panes(override_entry_animations: bool) -> SceneStrategyRef<&'static str> {
    scene_strategy(
        move |_: &SceneStrategyScope<&'static str>, entries: &[NavEntry<&'static str>]| {
            let [.., left, right] = entries else { return None };
            let scene = AnimatedTwoPanes {
                entries: [left.clone(), right.clone()],
                previous: Vec::new(),
                override_entry_animations,
            };
            Some(Rc::new(scene) as SceneRef<&'static str>)
        },
    )
}

fn scene_animated(
    override_entry_animations: bool,
    third_metadata: bool,
) -> (ComposeTestRule, OwnedStateList<&'static str>) {
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let ui = compose_test(move || {
        NavDisplay!(
            stack,
            scene_strategies = vec![animated_two_panes(override_entry_animations)],
            entry_provider = move |key: &'static str| {
                let entry = text_entry(key);
                if key == THIRD && third_metadata {
                    entry.with_metadata(NavDisplay::transition_spec(|_: Scope| {
                        Some(
                            slide_in_vertically_with(tween(SCENE_DURATION), |height| height / 2)
                                .together_with(ExitTransition::KEEP_UNTIL_TRANSITIONS_FINISHED),
                        )
                    }))
                } else {
                    entry
                }
            }
        );
    });
    (ui, owner)
}

fn position(ui: &mut ComposeTestRule, text: &str) -> (f32, f32) {
    let bounds = ui.on_node_with_text(text).fetch_semantics_snapshot().bounds;
    (bounds.left, bounds.top)
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testSceneAnimations
#[test]
fn a_scene_s_own_transition_runs() {
    let (mut ui, stack) = scene_animated(false, false);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    let width = ui.on_root().fetch_semantics_snapshot().bounds.width();
    ui.set_auto_advance(false);
    stack.push(THIRD);
    ui.advance_time_by(Duration::from_millis(u64::from(SCENE_DURATION) / 2));
    // The new scene's panes slide in from the right; the old one's left.
    let (third_x, third_y) = position(&mut ui, THIRD);
    assert!(third_x > width / 2.0, "{third_x}");
    assert_eq!(third_y, 0.0);
    assert!(position(&mut ui, FIRST).0 < 0.0);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(position(&mut ui, SECOND), (0.0, 0.0));
    assert_eq!(position(&mut ui, THIRD), (width / 2.0, 0.0));
    ui.on_node_with_text(FIRST).assert_does_not_exist();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testSceneOverridesEntryAnimations
#[test]
fn a_scene_can_go_over_its_entry_s_transition() {
    let (mut ui, stack) = scene_animated(true, true);
    ui.wait_for_idle();
    let width = ui.on_root().fetch_semantics_snapshot().bounds.width();
    ui.set_auto_advance(false);
    stack.push(THIRD);
    ui.advance_time_by(Duration::from_millis(u64::from(SCENE_DURATION) / 2));
    let (third_x, third_y) = position(&mut ui, THIRD);
    assert!(third_x > width / 2.0, "the scene's, horizontal: {third_x}");
    assert_eq!(third_y, 0.0);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testSceneDoesNotOverridesEntryAnimations
#[test]
fn a_scene_can_leave_it_to_its_entry_s_transition() {
    let (mut ui, stack) = scene_animated(false, true);
    ui.wait_for_idle();
    let width = ui.on_root().fetch_semantics_snapshot().bounds.width();
    ui.set_auto_advance(false);
    stack.push(THIRD);
    ui.advance_time_by(Duration::from_millis(u64::from(SCENE_DURATION) / 2));
    let (third_x, third_y) = position(&mut ui, THIRD);
    assert_eq!(third_x, width / 2.0, "the entry's, vertical");
    assert!(third_y > 0.0, "{third_y}");
    assert_eq!(position(&mut ui, FIRST).0, 0.0, "kept where it was");
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testSwapStack
#[test]
fn another_stack_slides_in_by_its_entry_s_transition() {
    let (one, two) = (mutable_state_list_from([FIRST]), mutable_state_list_from([SECOND]));
    let stacks = [one.handle(), two.handle()];
    let state = mutable_state_of(0);
    let read = state.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stacks[read.get()],
            entry_provider = |key: &'static str| {
                let entry = clicking_entry(key);
                if key == SECOND {
                    entry.with_metadata(NavDisplay::transition_spec(|_: Scope| {
                        Some(
                            slide_in_horizontally_with(tween(300), |width| width / 2)
                                .together_with(slide_out_horizontally_with(tween(300), |width| {
                                    -width / 2
                                })),
                        )
                    }))
                } else {
                    entry
                }
            }
        );
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.set(1);
    ui.advance_time_by(Duration::from_millis(150));
    assert!(position(&mut ui, SECOND).0 > 0.0);
    assert!(position(&mut ui, FIRST).0 < 0.0);
}

/// What's drawn with an alpha, sorted.
fn alphas(ui: &mut ComposeTestRule) -> Vec<f32> {
    let mut alphas: Vec<f32> = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            compose_ui::platform::DrawOp::SaveLayerAlpha { alpha, .. } => Some(*alpha),
            _ => None,
        })
        .collect();
    alphas.sort_by(f32::total_cmp);
    alphas
}

/// A display's default is a 700 ms fade: halfway through, both show, along `tween`'s easing. By
/// what's drawn: `capture_to_image` needs Skia, which only `compose-skia`'s tests link (D904).
#[test]
fn a_display_fades_by_default() {
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || NavDisplay!(stack, entry_provider = text_entry));
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.push(SECOND);
    ui.advance_time_by(Duration::from_millis(350));
    let middle = alphas(&mut ui);
    assert_eq!(middle.len(), 2, "{middle:?}");
    // `FastOutSlowIn` past a linear half: and the two sum to one, as they run together.
    assert!(middle[1] > 0.6 && middle[1] < 0.85, "{middle:?}");
    assert!((middle[0] + middle[1] - 1.0).abs() < 0.01, "{middle:?}");
    ui.advance_time_by(Duration::from_millis(400));
    assert!(alphas(&mut ui).iter().all(|alpha| *alpha > 0.99), "{:?}", alphas(&mut ui));
}
