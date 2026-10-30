//! Compose's `PullToRefreshBoxTest.kt`, `PullToRefreshIndicatorTest.kt` and
//! `PullToRefreshStateImplTest.kt`, adapted. The `LoadingIndicator` ones wait for
//! `loading-indicator`.

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::{LazyColumn, LazyListScope, ScrollModifiers as _, remember_scroll_state};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Dp, IntSize, LocalBoxFuture, Modifier, Offset, SemanticsProperties, UnitExt};

const TAG: &str = "PullRefresh";
const LAZY: &str = "lazy";
const THRESHOLD: f32 = 80.0;
const SLOP: f32 = 8.0;

/// A list to pull, in a box.
fn pull_box(
    state: Rc<dyn PullToRefreshState>,
    is_refreshing: MutableState<bool>,
    on_refresh: impl Fn() + 'static,
) -> ComposeTestRule {
    let on_refresh = Rc::new(on_refresh);
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), move || {
        let (state, on_refresh) = (state.clone(), on_refresh.clone());
        MaterialTheme!({
            let (state, on_refresh) = (state.clone(), on_refresh.clone());
            PullToRefreshBox!(
                is_refreshing.get(),
                move || on_refresh(),
                modifier = Modifier.test_tag(TAG),
                state = Some(state),
                |_: BoxScope| {
                    LazyColumn!(
                        modifier = Modifier.fill_max_size().test_tag(LAZY),
                        |scope: LazyListScope| {
                            scope.items(100, |_| {
                                Text!("item", modifier = Modifier.fill_max_width().height(40.dp()));
                            });
                        }
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    ui
}

fn pull(ui: &mut ComposeTestRule, by: f32) {
    ui.on_node_with_tag(LAZY).perform_touch_input(|touch| {
        touch.swipe(Offset::new(100.0, 0.0), Offset::new(100.0, by), 200);
    });
    ui.wait_for_idle();
}

fn refreshing_state(value: bool) -> (OwnedState<bool>, MutableState<bool>) {
    let owned = mutable_state_of(value);
    let handle = owned.handle();
    (owned, handle)
}

/// A state that only records, so where it's asked to go stays.
struct Recording {
    fraction: OwnedState<f32>,
}

impl Default for Recording {
    fn default() -> Recording {
        Recording { fraction: mutable_state_of(0.0) }
    }
}

impl PullToRefreshState for Recording {
    fn distance_fraction(&self) -> f32 {
        self.fraction.get()
    }
    fn is_animating(&self) -> bool {
        false
    }
    fn animate_to_threshold(&self) -> LocalBoxFuture<'_, ()> {
        Box::pin(async {})
    }
    fn animate_to_hidden(&self) -> LocalBoxFuture<'_, ()> {
        Box::pin(async {})
    }
    fn snap_to(&self, target_value: f32) -> LocalBoxFuture<'_, ()> {
        self.fraction.set(target_value);
        Box::pin(async {})
    }
}

/// Whose animations don't interrupt each other: the hide outlasts the threshold's.
struct NonInterrupting {
    fraction: OwnedState<f32>,
    animating: Cell<u32>,
}

impl NonInterrupting {
    async fn to(&self, target: f32, millis: u32) {
        self.animating.set(self.animating.get() + 1);
        let from = self.fraction.get();
        let mut start = None;
        loop {
            let now = with_frame_millis(|time| time).await;
            let begun = *start.get_or_insert(now);
            let along = ((now - begun) as f32 / millis as f32).min(1.0);
            self.fraction.set(from + (target - from) * along);
            if along >= 1.0 {
                break;
            }
        }
        self.animating.set(self.animating.get() - 1);
    }
}

impl PullToRefreshState for NonInterrupting {
    fn distance_fraction(&self) -> f32 {
        self.fraction.get()
    }
    fn is_animating(&self) -> bool {
        self.animating.get() > 0
    }
    fn animate_to_threshold(&self) -> LocalBoxFuture<'_, ()> {
        Box::pin(self.to(1.0, 100))
    }
    fn animate_to_hidden(&self) -> LocalBoxFuture<'_, ()> {
        Box::pin(self.to(0.0, 300))
    }
    fn snap_to(&self, target_value: f32) -> LocalBoxFuture<'_, ()> {
        self.fraction.set(target_value);
        Box::pin(async {})
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshBoxTest.kt, box_startRefreshing_updatesFraction, box_startNotRefreshing_updatesFraction
#[test]
fn the_fraction_starts_at_where_refreshing_says() {
    for (refreshing, fraction) in [(true, 1.0), (false, 0.0)] {
        let state = pull_to_refresh_state();
        let (_owned, flag) = refreshing_state(refreshing);
        let _ui = pull_box(state.clone(), flag, || {});
        assert_eq!(state.distance_fraction(), fraction);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshBoxTest.kt, boxNotVisible_notVisibleToSemantics, boxVisible_VisibleToSemantics
#[test]
fn the_indicator_is_a_progress_only_while_it_shows() {
    let state = pull_to_refresh_state();
    let (_owned, flag) = refreshing_state(false);
    let mut ui = pull_box(state, flag, || {});
    ui.on_node(key_is_defined(&SemanticsProperties::PROGRESS_BAR_RANGE_INFO))
        .assert_does_not_exist();
    let state = pull_to_refresh_state();
    let (_owned, flag) = refreshing_state(true);
    let mut ui = pull_box(state, flag, || {});
    ui.on_node(key_is_defined(&SemanticsProperties::PROGRESS_BAR_RANGE_INFO)).assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshBoxTest.kt, startRefreshing_pull_isNoop
#[test]
fn refreshing_a_pull_does_nothing() {
    let state = pull_to_refresh_state();
    let (_owned, flag) = refreshing_state(true);
    let mut ui = pull_box(state.clone(), flag, || {});
    pull(&mut ui, THRESHOLD * 2.0 + SLOP);
    assert_eq!(state.distance_fraction(), 1.0);
    // Nor a shorter one, which would show.
    pull(&mut ui, THRESHOLD + SLOP);
    assert_eq!(state.distance_fraction(), 1.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshBoxTest.kt, startIdle_pull_triggersRefresh
#[test]
fn a_pull_past_the_threshold_refreshes() {
    let state = pull_to_refresh_state();
    let (_owned, flag) = refreshing_state(false);
    let mut ui = pull_box(state.clone(), flag, move || flag.set(true));
    assert_eq!(state.distance_fraction(), 0.0);
    pull(&mut ui, THRESHOLD * 2.0 + SLOP + 10.0);
    assert!(flag.get());
    assert_eq!(state.distance_fraction(), 1.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshBoxTest.kt, startIdle_pull_isRefreshingStaysFalse_resetsFraction
#[test]
fn if_refreshing_stays_false_it_goes_back() {
    let state = pull_to_refresh_state();
    let (_owned, flag) = refreshing_state(false);
    let count = Rc::new(Cell::new(0));
    let counted = count.clone();
    let mut ui = pull_box(state.clone(), flag, move || counted.set(counted.get() + 1));
    pull(&mut ui, THRESHOLD * 2.0 + SLOP + 10.0);
    assert_eq!(count.get(), 1);
    assert_eq!(state.distance_fraction(), 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshBoxTest.kt, startIdle_pull_customState_refreshStarts_staysAtThreshold
#[test]
fn with_a_state_of_its_own_a_refresh_stays_at_the_threshold() {
    let state =
        Rc::new(NonInterrupting { fraction: mutable_state_of(0.0), animating: Cell::new(0) });
    let (_owned, flag) = refreshing_state(false);
    let mut ui = pull_box(state.clone(), flag, move || flag.set(true));
    pull(&mut ui, THRESHOLD * 2.0 + SLOP + 10.0);
    assert!(flag.get());
    assert_eq!(state.distance_fraction(), 1.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshStateImplTest.kt, refreshTrigger_onlyAfterThreshold
#[test]
fn only_past_the_threshold_it_refreshes() {
    let state = pull_to_refresh_state();
    let (_owned, flag) = refreshing_state(false);
    let count = Rc::new(Cell::new(0));
    let counted = count.clone();
    let mut ui = pull_box(state.clone(), flag, move || counted.set(counted.get() + 1));
    pull(&mut ui, 2.0 * THRESHOLD + SLOP - 1.0);
    pull(&mut ui, 2.0 * THRESHOLD + SLOP);
    assert_eq!(count.get(), 0);
    assert_eq!(state.distance_fraction(), 0.0);
    pull(&mut ui, 2.0 * THRESHOLD + SLOP + 1.0);
    assert_eq!(count.get(), 1);
    assert_eq!(state.distance_fraction(), 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshStateImplTest.kt, progressAndVerticalOffset_scaleCorrectly_untilThreshold, progressAndPosition_scaleCorrectly_beyondThreshold, positionIsCapped
#[test]
fn the_fraction_scales_and_is_capped() {
    let run = |by: f32| {
        let state = Rc::new(Recording::default());
        let (_owned, flag) = refreshing_state(false);
        let count = Rc::new(Cell::new(0));
        let counted = count.clone();
        let mut ui = pull_box(state.clone(), flag, move || counted.set(counted.get() + 1));
        pull(&mut ui, by);
        (state.distance_fraction(), count.get())
    };
    assert_eq!(run(THRESHOLD + SLOP), (0.5, 0));
    assert_eq!(run(THRESHOLD * 2.0 + SLOP), (1.0, 0));
    let (fraction, count) = run(THRESHOLD * 3.0 + SLOP);
    assert!((fraction - 1.5).abs() < 0.1, "{fraction}");
    assert_eq!(count, 1);
    assert_eq!(run(THRESHOLD * 10.0 + SLOP).0, 2.0);
}

/// Not enabled, a pull is the list's alone.
#[test]
fn disabled_it_stays() {
    let state = Rc::new(Recording::default());
    let given = state.clone();
    let count = Rc::new(Cell::new(0));
    let counted = count.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), move || {
        let (given, counted) = (given.clone(), counted.clone());
        MaterialTheme!({
            let (given, counted) = (given.clone(), counted.clone());
            PullToRefreshBox!(
                false,
                move || counted.set(counted.get() + 1),
                state = Some(given as Rc<dyn PullToRefreshState>),
                enabled = false,
                |_: BoxScope| {
                    Box!(
                        modifier = Modifier
                            .fill_max_size()
                            .test_tag(LAZY)
                            .vertical_scroll(remember_scroll_state(0))
                    );
                }
            );
        });
    });
    pull(&mut ui, THRESHOLD * 3.0);
    assert_eq!(state.distance_fraction(), 0.0);
    assert_eq!(count.get(), 0);
}

/// A threshold of its own.
#[test]
fn it_takes_a_threshold() {
    let state = Rc::new(Recording::default());
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), move || {
        let given = given.clone();
        MaterialTheme!({
            let given = given.clone();
            PullToRefreshBox!(
                false,
                || {},
                state = Some(given as Rc<dyn PullToRefreshState>),
                threshold = Dp(40.0),
                |_: BoxScope| {
                    Box!(
                        modifier = Modifier
                            .fill_max_size()
                            .test_tag(LAZY)
                            .vertical_scroll(remember_scroll_state(0))
                    );
                }
            );
        });
    });
    pull(&mut ui, 40.0 + SLOP);
    assert_eq!(state.distance_fraction(), 0.5);
}

struct RecordingFling {
    velocity: Rc<Cell<f32>>,
}

impl compose_foundation::gestures::FlingBehavior for RecordingFling {
    fn perform_fling<'a>(
        &'a self,
        _scope: compose_foundation::gestures::ScrollScope,
        initial_velocity: f32,
    ) -> LocalBoxFuture<'a, f32> {
        self.velocity.set(initial_velocity);
        Box::pin(async move { initial_velocity })
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshBoxTest.kt, box_fling_isConsumed
#[test]
fn the_fling_of_a_pull_is_taken() {
    let state = pull_to_refresh_state();
    let given = state.clone();
    let remaining = Rc::new(Cell::new(f32::NAN));
    let velocity = remaining.clone();
    let (_owned, flag) = refreshing_state(false);
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), move || {
        let (given, velocity) = (given.clone(), velocity.clone());
        MaterialTheme!({
            let (given, velocity) = (given.clone(), velocity.clone());
            PullToRefreshBox!(
                flag.get(),
                move || flag.set(true),
                state = Some(given),
                |_: BoxScope| {
                    let fling = Rc::new(RecordingFling { velocity: velocity.clone() });
                    Column!(
                        modifier = Modifier.fill_max_width().test_tag(LAZY).vertical_scroll_with(
                            remember_scroll_state(0),
                            true,
                            Some(fling),
                            false
                        ),
                        {
                            for _ in 0..50 {
                                Text!("Lorem ipsum", modifier = Modifier.height(40.dp()));
                            }
                        }
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    // Down fast, not to the threshold: nothing's left for the content to fling with.
    ui.on_node_with_tag(LAZY).perform_touch_input(|touch| {
        touch.swipe(Offset::new(100.0, 0.0), Offset::new(100.0, THRESHOLD / 2.0), 100);
    });
    ui.wait_for_idle();
    assert_eq!(state.distance_fraction(), 0.0);
    assert_eq!(remaining.get().abs(), 0.0);
}

/// A pull down then up on the same finger gives it back to the list.
#[test]
fn pulling_back_up_hides_it_before_the_list_scrolls() {
    let state = Rc::new(Recording::default());
    let (_owned, flag) = refreshing_state(false);
    let mut ui = pull_box(state.clone(), flag, || {});
    ui.on_node_with_tag(LAZY).perform_touch_input(|touch| {
        touch.down(Offset::new(100.0, 10.0));
        for _ in 0..10 {
            touch.move_by(Offset::new(0.0, 10.0));
        }
        for _ in 0..4 {
            touch.move_by(Offset::new(0.0, -10.0));
        }
    });
    ui.wait_for_idle();
    let fraction = state.distance_fraction();
    assert!((fraction - (100.0 - SLOP - 40.0) * 0.5 / THRESHOLD).abs() < 0.1, "{fraction}");
    ui.on_node_with_tag(LAZY).perform_touch_input(|touch| touch.up());
}

#[composable]
fn Indicator(state: Rc<dyn PullToRefreshState>, is_refreshing: bool) {
    MaterialTheme!({
        let state = state.clone();
        Box!(modifier = Modifier.fill_max_size(), {
            PullToRefreshIndicator!(
                state.clone(),
                is_refreshing,
                modifier = Modifier.test_tag("indicator")
            );
        });
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshIndicatorTest.kt, indicatorDisplayed_refreshing, indicatorDisplayed_notRefreshing
#[test]
fn the_indicator_is_displayed() {
    for refreshing in [true, false] {
        let state = pull_to_refresh_state();
        let mut ui = compose_test(move || {
            let state = state.clone();
            Indicator!(state, refreshing);
        });
        ui.wait_for_idle();
        ui.on_node_with_tag("indicator").assert_is_displayed();
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshIndicatorTest.kt, indicatorRespects_changingOffset
#[test]
fn the_indicator_goes_by_the_state() {
    let state = pull_to_refresh_state();
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), move || {
        let given = given.clone();
        MaterialTheme!({
            let given = given.clone();
            let kept_for_effect = given.clone();
            Box!(modifier = Modifier.fill_max_size(), {
                PullToRefreshIndicator!(
                    given.clone(),
                    true,
                    modifier = Modifier.size(30.dp()).test_tag("indicator"),
                    max_distance = Dp(200.0)
                );
            });
            let animated = kept_for_effect.clone();
            LaunchedEffect((), async move { animated.animate_to_threshold().await });
        });
    });
    ui.wait_for_idle();
    let top_of_layer = |ui: &mut ComposeTestRule| {
        let ops = ui.host().display_list().ops.clone();
        ops.iter().find_map(|op| match op {
            compose_ui::platform::DrawOp::Transform { translation, .. } => Some(translation.y),
            _ => None,
        })
    };
    assert_eq!(top_of_layer(&mut ui), Some(200.0 - 30.0));
    SnapNow::run(&state, 0.5);
    ui.wait_for_idle();
    assert_eq!(top_of_layer(&mut ui), Some(100.0 - 30.0));
}

struct SnapNow;

impl SnapNow {
    /// `runBlocking { state.snapTo(fraction) }`: the default state's is immediate.
    fn run(state: &Rc<dyn PullToRefreshState>, fraction: f32) {
        let future = state.snap_to(fraction);
        let mut future = std::pin::pin!(future);
        let waker = std::task::Waker::noop();
        let mut context = std::task::Context::from_waker(waker);
        assert!(future.as_mut().poll(&mut context).is_ready());
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshIndicatorTest.kt, indicatorDoesNotCapturePointerEvents
#[test]
fn the_indicator_takes_no_presses() {
    let pressed = Rc::new(Cell::new(0));
    let counted = pressed.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), move || {
        let counted = counted.clone();
        MaterialTheme!({
            let counted = counted.clone();
            Box!({
                let counted = counted.clone();
                Box!(
                    modifier =
                        Modifier.fill_max_size().clickable(move || counted.set(counted.get() + 1))
                );
                let state = pull_to_refresh_state_kept();
                PullToRefreshIndicator!(state, true, modifier = Modifier.test_tag("indicator"));
            });
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("indicator").perform_click();
    assert_eq!(pressed.get(), 1);
}

fn pull_to_refresh_state_kept() -> Rc<dyn PullToRefreshState> {
    remember_pull_to_refresh_state()
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshStateImplTest.kt, state_restoresPullRefreshState
#[test]
fn the_state_is_restored() {
    let seen = Rc::new(std::cell::RefCell::new(None));
    let kept = seen.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), || {});
    let mut tester = StateRestorationTester::new();
    tester.set_content(&mut ui, move || {
        *kept.borrow_mut() = Some(remember_pull_to_refresh_state());
    });
    let state = seen.borrow().clone().unwrap();
    SnapNow::run(&state, 0.5);
    tester.emulate_save_and_restore(&mut ui);
    assert_eq!(seen.borrow().as_ref().unwrap().distance_fraction(), 0.5);
}

/// The values, by their tokens.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/pulltorefresh/PullToRefresh.kt, PullToRefreshDefaults
#[test]
fn the_defaults() {
    assert_eq!(PullToRefreshDefaults::POSITIONAL_THRESHOLD, 80.dp());
    assert_eq!(PullToRefreshDefaults::INDICATOR_MAX_DISTANCE, 80.dp());
    assert_eq!(PullToRefreshDefaults::ELEVATION, 3.dp());
    let _ui = compose_test(|| {
        MaterialTheme!({
            let scheme = MaterialTheme::color_scheme();
            assert_eq!(
                PullToRefreshDefaults::indicator_container_color(),
                scheme.surface_container_high
            );
            assert_eq!(PullToRefreshDefaults::indicator_color(), scheme.on_surface_variant);
        });
    });
}

/// A column of 2000 in a box in 800, a state that records, and what's scrolled.
fn scrolling_box(
    refreshing: MutableState<bool>,
    threshold: MutableState<Dp>,
) -> (ComposeTestRule, Rc<Recording>, compose_foundation::ScrollState) {
    let state = Rc::new(Recording::default());
    let given = state.clone();
    let out = Rc::new(Cell::new(None));
    let kept = out.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), move || {
        let (given, kept) = (given.clone(), kept.clone());
        MaterialTheme!({
            let (given, kept) = (given.clone(), kept.clone());
            PullToRefreshBox!(
                refreshing.get(),
                || {},
                state = Some(given as Rc<dyn PullToRefreshState>),
                threshold = threshold.get(),
                |_: BoxScope| {
                    let scroll = remember_scroll_state(0);
                    kept.set(Some(scroll));
                    Box!(
                        modifier = Modifier.fill_max_size().test_tag(LAZY).vertical_scroll(scroll),
                        {
                            Box!(modifier = Modifier.fill_max_width().height(2000.dp()));
                        }
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    let scroll = out.get().unwrap();
    (ui, state, scroll)
}

/// Up, a list scrolls: there's nothing pulled to take it back from.
#[test]
fn an_upward_swipe_is_the_lists() {
    let (_flag_owned, flag) = refreshing_state(false);
    let threshold_owned = mutable_state_of(Dp(80.0));
    let (mut ui, state, scroll) = scrolling_box(flag, threshold_owned.handle());
    ui.on_node_with_tag(LAZY).perform_touch_input(|touch| {
        touch.swipe(Offset::new(100.0, 400.0), Offset::new(100.0, 100.0), 300);
    });
    ui.wait_for_idle();
    assert!(scroll.value() > 200, "{}", scroll.value());
    assert_eq!(state.distance_fraction(), 0.0);
}

/// A fling reaching the top isn't a pull, however fast.
#[test]
fn a_fling_reaching_the_top_does_not_pull() {
    let (_flag_owned, flag) = refreshing_state(false);
    let threshold_owned = mutable_state_of(Dp(80.0));
    let (mut ui, state, scroll) = scrolling_box(flag, threshold_owned.handle());
    ui.on_node_with_tag(LAZY).perform_touch_input(|touch| {
        touch.swipe(Offset::new(100.0, 400.0), Offset::new(100.0, 300.0), 500);
    });
    ui.wait_for_idle();
    let before = scroll.value();
    assert!(before > 50, "{before}");
    // Fast, a little down, not to the top: the fling reaches it, and has velocity left there.
    ui.on_node_with_tag(LAZY).perform_touch_input(|touch| {
        touch.swipe(Offset::new(100.0, 100.0), Offset::new(100.0, 140.0), 16);
    });
    ui.wait_for_idle();
    assert_eq!(scroll.value(), 0);
    assert_eq!(state.distance_fraction(), 0.0);
}

/// A new threshold counts from the next pull.
#[test]
fn the_threshold_can_change() {
    let (_flag_owned, flag) = refreshing_state(false);
    let threshold_owned = mutable_state_of(Dp(80.0));
    let threshold = threshold_owned.handle();
    let (mut ui, state, _scroll) = scrolling_box(flag, threshold);
    threshold.set(Dp(40.0));
    ui.wait_for_idle();
    pull(&mut ui, 40.0 + SLOP);
    assert_eq!(state.distance_fraction(), 0.5);
}

/// What the indicator draws: its layer's clip and shadow while it shows, the arc and the arrow
/// at 30 % until the threshold, clipped at the top.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/pulltorefresh/PullToRefresh.kt, IndicatorBox, CircularArrowProgressIndicator
#[test]
fn what_the_indicator_draws() {
    use compose_ui::platform::DrawOp;
    let (_flag_owned, flag) = refreshing_state(false);
    let threshold_owned = mutable_state_of(Dp(80.0));
    let (mut ui, state, _scroll) = scrolling_box(flag, threshold_owned.handle());
    let shadow = |ui: &mut ComposeTestRule| {
        let ops = ui.host().display_list().ops.clone();
        ops.iter().any(|op| matches!(op, DrawOp::DrawShadow { .. }))
    };
    assert!(!shadow(&mut ui));
    pull(&mut ui, THRESHOLD + SLOP);
    assert!(shadow(&mut ui));
    assert_eq!(state.distance_fraction(), 0.5);
    let ops = ui.host().display_list().ops.clone();
    let arc = ops
        .iter()
        .find_map(|op| match op {
            DrawOp::DrawArc { paint, .. } => Some(paint.color),
            _ => None,
        })
        .expect("an arc");
    assert_eq!((arc.alpha() as f32 / 255.0 * 10.0).round(), 3.0);
    assert!(ops.iter().any(|op| matches!(op, DrawOp::DrawPath { .. })), "the arrow");
    assert!(
        ops.iter()
            .any(|op| matches!(op, DrawOp::ClipRect(rect) if rect.top == 0.0 && rect.left < -1e6)),
        "clipped at the top: {ops:?}"
    );
    assert!(ops.iter().any(|op| matches!(op, DrawOp::ClipRoundRect(..))), "the layer's clip");
}

/// Refreshing, it's a spinner, with no progress to tell; pulled, the progress.
#[test]
fn refreshing_it_is_a_spinner() {
    let state = pull_to_refresh_state();
    let (_owned, flag) = refreshing_state(true);
    let mut ui = pull_box(state, flag, || {});
    let node = ui
        .on_node(key_is_defined(&SemanticsProperties::PROGRESS_BAR_RANGE_INFO))
        .fetch_semantics_node();
    let info = *node.config.get(&SemanticsProperties::PROGRESS_BAR_RANGE_INFO).unwrap();
    assert_eq!(info, compose_ui::ProgressBarRangeInfo::INDETERMINATE);
}

/// The default indicator's at the top center.
#[test]
fn the_default_indicator_is_at_the_top_center() {
    let state = pull_to_refresh_state();
    let (_owned, flag) = refreshing_state(true);
    let mut ui = pull_box(state, flag, || {});
    let node = ui
        .on_node_unmerged(key_is_defined(&SemanticsProperties::PROGRESS_BAR_RANGE_INFO))
        .fetch_semantics_node();
    assert_eq!((node.bounds.left + node.bounds.right) / 2.0, 200.0);
}

/// Refreshing, a fling down's the list's whatever's been pulled; idle with a pull showing, it's
/// taken, the content's left with none.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/pulltorefresh/PullToRefreshBoxTest.kt, box_fling_isConsumed
#[test]
fn a_fling_is_taken_only_with_a_pull_showing() {
    let (_flag_owned, flag) = refreshing_state(false);
    let threshold_owned = mutable_state_of(Dp(80.0));
    let (mut ui, _state, scroll) = scrolling_box(flag, threshold_owned.handle());
    ui.on_node_with_tag(LAZY).perform_touch_input(|touch| {
        touch.swipe(Offset::new(100.0, 700.0), Offset::new(100.0, 100.0), 300);
    });
    ui.wait_for_idle();
    let before = scroll.value();
    assert!(before > 600);
    // Refreshing: the state's at the threshold, what's pulled too.
    flag.set(true);
    ui.wait_for_idle();
    ui.on_node_with_tag(LAZY).perform_touch_input(|touch| {
        touch.swipe(Offset::new(100.0, 300.0), Offset::new(100.0, 400.0), 30);
    });
    ui.wait_for_idle();
    assert!(before - scroll.value() > 100, "it flung on: {before}, {}", scroll.value());
}
