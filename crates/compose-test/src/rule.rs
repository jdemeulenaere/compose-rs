//! The test rule: a host on the test clock, its idling, and its finders.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_runtime::platform;
use compose_ui::{
    Density, Dp, IntSize, Key, KeyEvent,
    platform::{FixedTextMeasurer, TextMeasurer, UiHost},
};

use crate::injection::InputState;
use crate::interaction::{SemanticsNodeInteraction, SemanticsNodeInteractionCollection};
use crate::matcher::{Matcher, TreeView, has_content_description, has_test_tag, has_text, is_root};
use crate::selector::Selector;
use crate::{default_host, thread_optimizations};

/// One frame's duration on the test clock, as Compose's test frame clock (16 ms).
pub const FRAME_DURATION: Duration = Duration::from_millis(16);

/// The timeout `wait_until` and `advance_time_until` use unless given one, as Compose's (1 s).
pub const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(1);

/// The tolerance of bounds assertions given none, as Compose's (0.5 dp).
pub const DEFAULT_BOUNDS_TOLERANCE: Dp = Dp(0.5);

/// How long `wait_for_idle` waits (the test clock running animations, or idling resources
/// staying busy) before deciding the UI never idles, unless told otherwise
/// (`ComposeUiTestConfig::test_timeout`).
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Something outside compose-rs the test waits for, as Compose's `IdlingResource`: waiting for
/// idle also waits until each registered one is idle.
pub trait IdlingResource {
    /// Whether it's idle now, as `isIdleNow`.
    fn is_idle_now(&self) -> bool;
    /// What it's busy with, for the failure when it never idles, as
    /// `getDiagnosticMessageIfBusy`.
    fn diagnostic_message_if_busy(&self) -> Option<String> {
        None
    }
}

/// A UI under test, as Compose's `ComposeTestRule`: owns a headless host on a test clock, and
/// waits for idle after every action and before every finder's look at the tree.
pub struct ComposeTestRule {
    pub(crate) host: UiHost,
    /// `mainClock.autoAdvance`, shared with the infinite-animation policy the rule installs.
    auto_advance: Rc<Cell<bool>>,
    /// The test clock's time when the rule started: `current_time_millis` counts from it, as
    /// Compose's `mainClock.currentTime` from 0.
    origin_nanos: u64,
    /// When the next frame runs on the test clock, once one is needed: a frame comes one frame
    /// duration after the first need since the last frame, as Compose's
    /// `TestMonotonicFrameClock` (D900).
    next_frame: Option<u64>,
    /// The policy installed for infinite animations (D901), removed with the rule.
    policy: Rc<dyn Fn() -> bool>,
    /// Whether the rule set the thread's motion duration scale, put back to 1 with it (D827).
    scaled: bool,
    /// Whether finders wait for idle first, as Compose's implicit wait
    /// (`run_without_implicit_wait` turns it off for its block).
    implicit_wait: bool,
    /// The bounds assertions' tolerance given none, as `ComposeUiTestConfig`'s
    /// `boundsAssertionTolerance`.
    bounds_tolerance: Dp,
    /// The injected pointers and keys, kept across `perform_*_input` blocks, as Compose's
    /// `InputDispatcher` state.
    pub(crate) input: InputState,
    /// How long one wait for idle may take, in test time (animations) or real time (idling
    /// resources): `DEFAULT_IDLE_TIMEOUT`.
    idle_timeout: Duration,
    /// When the whole test's real time is up, as Compose's `testTimeout`
    /// (`ComposeUiTestConfig::test_timeout`, D908): waits past it fail.
    test_deadline: Option<(std::time::Instant, Duration)>,
    /// What waiting for idle also waits for, as `registerIdlingResource`.
    idling_resources: Vec<Rc<dyn IdlingResource>>,
}

/// A test rule composing `content` in the default root, measuring text with the fixed-advance
/// measurer.
pub fn compose_test(content: impl Fn() + 'static) -> ComposeTestRule {
    ComposeTestRule::with_host(default_host(), content)
}

/// A test rule with a root of `size` pixels at `density`, measuring text with `text_measurer`.
pub fn compose_test_with(
    size: IntSize,
    density: Density,
    text_measurer: Box<dyn TextMeasurer>,
    content: impl Fn() + 'static,
) -> ComposeTestRule {
    ComposeTestRule::with_host(UiHost::new(size, density, text_measurer), content)
}

impl ComposeTestRule {
    /// A rule over `host` with no content yet, as `createComposeRule()` before `setContent`:
    /// set the clock up (`set_auto_advance`), then [`set_content`](Self::set_content).
    pub fn empty(host: UiHost) -> ComposeTestRule {
        // What compose-rs logs (a stale handle's warning) shows in the test's output.
        compose_ui::platform::install_stderr_logger();
        platform::track_recompositions(true);
        // In debug builds, each semantics tree checks the configurations kept per node first: a
        // missed invalidation fails the test (D1261). Not in a benchmark's profile: it measures
        // the product.
        compose_ui::platform::set_semantics_cache_check(cfg!(debug_assertions));
        // The test's main clock: time moves only as the rule advances it.
        let origin_nanos = platform::now_nanos();
        platform::set_manual_clock(origin_nanos);
        let auto_advance = Rc::new(Cell::new(true));
        // Infinite animations don't run while the clock advances by itself, as Compose's test
        // environment's `InfiniteAnimationPolicy` (D901).
        let policy: Rc<dyn Fn() -> bool> = {
            let auto_advance = auto_advance.clone();
            Rc::new(move || !auto_advance.get())
        };
        platform::set_infinite_animation_policy(Some(policy.clone()));
        let mut rule = ComposeTestRule {
            host,
            auto_advance,
            origin_nanos,
            next_frame: None,
            policy,
            scaled: false,
            implicit_wait: true,
            bounds_tolerance: DEFAULT_BOUNDS_TOLERANCE,
            input: InputState::default(),
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            test_deadline: None,
            idling_resources: Vec::new(),
        };
        if let Some(optimizations) = thread_optimizations() {
            rule.host.set_optimizations(optimizations);
        }
        rule
    }

    /// Composes `content`, at once and without moving the clock, as `setContent`, then waits
    /// for idle. A rule takes content once.
    pub fn set_content(&mut self, content: impl Fn() + 'static) {
        self.host.set_content(content);
        self.wait_for_idle();
    }

    pub fn with_host(host: UiHost, content: impl Fn() + 'static) -> ComposeTestRule {
        let mut rule = ComposeTestRule::empty(host);
        rule.set_content(content);
        rule
    }

    /// A test rule whose clock moves only by `advance_time_by`, as a test that sets
    /// `mainClock.autoAdvance = false` before its content: frames, animations and delays wait
    /// for it.
    pub fn with_manual_clock(content: impl Fn() + 'static) -> ComposeTestRule {
        let mut rule = ComposeTestRule::empty(default_host());
        rule.set_auto_advance(false);
        rule.set_content(content);
        rule
    }

    /// A rule over the fixed-advance measurer at density 1, in a root of `size`.
    pub fn sized(size: IntSize, content: impl Fn() + 'static) -> ComposeTestRule {
        compose_test_with(size, Density::default(), Box::new(FixedTextMeasurer::new()), content)
    }

    pub fn host(&mut self) -> &mut UiHost {
        &mut self.host
    }

    /// The root's density, as the rule's `density`.
    pub fn density(&self) -> Density {
        self.host.with_tree(|tree| tree.density())
    }

    /// The tolerance of bounds assertions given none (0.5 dp by default), as
    /// `ComposeUiTestConfig(boundsAssertionTolerance)`.
    pub fn set_bounds_assertion_tolerance(&mut self, tolerance: Dp) {
        self.bounds_tolerance = tolerance;
    }

    pub fn bounds_assertion_tolerance(&self) -> Dp {
        self.bounds_tolerance
    }

    // ---- Idling ----------------------------------------------------------------------------

    /// Whether the UI needs a frame of the test clock: a composition to recompose, or a task
    /// waiting for a frame (an animation). Other work (tasks, layout, drawing, focus) runs
    /// without one, as Compose's measure and layout passes run outside the test clock.
    fn needs_clock_frame(&self) -> bool {
        self.host.composition().has_invalidations() || platform::has_frame_awaiters()
    }

    /// Runs what's due at the clock's time without a frame of the clock (Compose's
    /// `runCurrent` with the passes that follow it): timers due, ready tasks, and layout,
    /// drawing and focus work that needs no recomposition. A need for a frame schedules one,
    /// a frame duration from now.
    pub(crate) fn run_current(&mut self) {
        for _ in 0..10_000 {
            platform::fire_timers();
            platform::run_ready();
            // Focus moves happen at once; what they call writes for the next frame (D906).
            self.host.handle_focus_requests();
            platform::run_ready();
            if self.needs_clock_frame() {
                if self.next_frame.is_none() {
                    self.next_frame =
                        Some(platform::now_nanos() + FRAME_DURATION.as_nanos() as u64);
                }
                return;
            }
            if !self.host.has_pending_work() {
                return;
            }
            // Nothing to recompose nor to resume: a pass at the current time.
            self.host.frame();
        }
        panic!("compose-test: the UI never settled: a pass that always asks for another?");
    }

    /// Moves the clock to `end` (a time of the executor's clock), running the timers and frames
    /// due on the way, in time order.
    pub(crate) fn advance_clock_to(&mut self, end: u64) {
        self.run_current();
        loop {
            let now = platform::now_nanos();
            let timer = platform::next_timer_deadline().filter(|deadline| *deadline > now);
            let next = [timer, self.next_frame].into_iter().flatten().fold(end, u64::min);
            if next > now {
                platform::advance_clock(Duration::from_nanos(next - now));
            }
            if self.next_frame.is_some_and(|frame| frame <= next) {
                // The tasks due at the frame's time run before it.
                platform::fire_timers();
                platform::run_ready();
                self.next_frame = None;
                self.host.frame();
            }
            self.run_current();
            if next >= end {
                return;
            }
        }
    }

    /// Waits until the UI is idle, as `waitForIdle`: runs what's due now, and while a frame is
    /// needed (a recomposition, an animation) and the clock advances by itself (the default, as
    /// `mainClock.autoAdvance`), moves the clock a frame at a time. Pending `delay`s don't keep
    /// it busy, as in Compose: advance the clock past them (D900). With the clock advanced by
    /// hand, a needed frame waits for `advance_time_by`.
    pub fn wait_for_idle(&mut self) {
        let start = platform::now_nanos();
        let started = std::time::Instant::now();
        let mut frames = 0;
        loop {
            self.check_test_deadline();
            self.run_current();
            let frame_due = self.next_frame.is_some() && self.auto_advance.get();
            let busy: Vec<Option<String>> = self
                .idling_resources
                .iter()
                .filter(|resource| !resource.is_idle_now())
                .map(|resource| resource.diagnostic_message_if_busy())
                .collect();
            if !frame_due && busy.is_empty() {
                return;
            }
            let elapsed = Duration::from_nanos(platform::now_nanos() - start);
            let waited = started.elapsed();
            if elapsed > self.idle_timeout || waited > self.idle_timeout || frames >= 100_000 {
                let reasons: Vec<String> = busy.into_iter().flatten().collect();
                panic!(
                    "compose-test: ComposeNotIdleException: the UI never became idle ({frames} \
                     frames, {elapsed:?} of test time{}){}",
                    if frame_due {
                        ": an animation or a recomposition that never ends?"
                    } else {
                        ""
                    },
                    if reasons.is_empty() {
                        String::new()
                    } else {
                        format!("; busy idling resources: {}", reasons.join("; "))
                    }
                );
            }
            if frame_due {
                // A frame needed: the clock moves a frame on, as Compose's idling.
                self.advance_clock_to(platform::now_nanos() + FRAME_DURATION.as_nanos() as u64);
            } else {
                // Only idling resources busy: real time passes for them, the clock stays, as
                // Espresso's wait.
                std::thread::sleep(Duration::from_millis(10));
            }
            frames += 1;
        }
    }

    /// Waits for idle, as `awaitIdle`: Rust tests aren't suspended, so it's `wait_for_idle`
    /// behind an `async fn`, for code written against Compose's suspending form.
    pub async fn await_idle(&mut self) {
        self.wait_for_idle();
    }

    /// Whether the UI has work left, without doing any or moving the clock, as
    /// `hasPendingWork`: a frame needed (a recomposition, an animation), a task ready, a pass
    /// due (layout, drawing, focus).
    pub fn has_pending_work(&self) -> bool {
        self.next_frame.is_some()
            || self.needs_clock_frame()
            || self.host.has_pending_work()
            || platform::has_ready_tasks()
    }

    /// Waiting for idle also waits until `resource` is idle, as `registerIdlingResource`.
    pub fn register_idling_resource(&mut self, resource: Rc<dyn IdlingResource>) {
        if !self.idling_resources.iter().any(|known| Rc::ptr_eq(known, &resource)) {
            self.idling_resources.push(resource);
        }
    }

    /// As `unregisterIdlingResource`.
    pub fn unregister_idling_resource(&mut self, resource: &Rc<dyn IdlingResource>) {
        self.idling_resources.retain(|known| !Rc::ptr_eq(known, resource));
    }

    /// How long one wait for idle may take before it fails (one minute by default): of test
    /// time while frames are needed (an animation that never ends), of real time while idling
    /// resources are busy.
    pub fn set_idle_timeout(&mut self, timeout: Duration) {
        self.idle_timeout = timeout;
    }

    /// The whole test's real-time timeout from now, as Compose's `testTimeout`: waits after it's
    /// past fail (`run_compose_ui_test_with` sets the config's).
    pub fn set_test_timeout(&mut self, timeout: Duration) {
        self.test_deadline = Some((std::time::Instant::now() + timeout, timeout));
    }

    /// Lifts the test's timeout (for the failure pipeline, which runs after it).
    pub(crate) fn clear_test_timeout(&mut self) {
        self.test_deadline = None;
    }

    /// Panics if the test's real time is up, as Compose's `testTimeout`.
    #[track_caller]
    pub(crate) fn check_test_deadline(&self) {
        if let Some((deadline, timeout)) = self.test_deadline
            && std::time::Instant::now() > deadline
        {
            panic!(
                "compose-test: ComposeTimeoutException: the test did not complete within its testTimeout of {timeout:?}"
            );
        }
    }

    /// Waits for idle unless in [`run_without_implicit_wait`](Self::run_without_implicit_wait):
    /// what finders do before looking at the tree, as Compose's.
    pub(crate) fn implicit_wait(&mut self) {
        if self.implicit_wait {
            self.wait_for_idle();
        }
    }

    /// Waits for idle, then runs `action` with the rule, as `runOnIdle`.
    pub fn run_on_idle<R>(&mut self, action: impl FnOnce(&mut Self) -> R) -> R {
        self.wait_for_idle();
        action(self)
    }

    /// Runs `action` with the rule, as `runOnUiThread`: the test thread is the UI thread.
    pub fn run_on_ui_thread<R>(&mut self, action: impl FnOnce(&mut Self) -> R) -> R {
        action(self)
    }

    /// Runs `action` with finders that don't wait for idle first, as `runWithoutImplicitWait`.
    pub fn run_without_implicit_wait<R>(&mut self, action: impl FnOnce(&mut Self) -> R) -> R {
        let previous = std::mem::replace(&mut self.implicit_wait, false);
        let result = action(self);
        self.implicit_wait = previous;
        result
    }

    /// Waits until `condition` holds, as `waitUntil` with its default timeout (1 s): checks it,
    /// and while it doesn't hold, moves the clock a frame on (when it advances by itself) or
    /// lets background threads work for 10 ms. Panics after the timeout, as
    /// `ComposeTimeoutException`.
    #[track_caller]
    pub fn wait_until(&mut self, condition: impl FnMut(&mut Self) -> bool) {
        self.wait_until_with(None, DEFAULT_WAIT_TIMEOUT, condition);
    }

    /// The same, with a `timeout`, and a description of the condition for the failure, as
    /// `waitUntil(conditionDescription, timeoutMillis)`. Compose counts real time, 10 ms a
    /// check; this counts the checks, so it is deterministic.
    #[track_caller]
    pub fn wait_until_with(
        &mut self,
        description: Option<&str>,
        timeout: Duration,
        mut condition: impl FnMut(&mut Self) -> bool,
    ) {
        let checks = (timeout.as_millis() / 10).max(1);
        let mut checked = 0;
        self.run_current();
        while !condition(self) {
            self.check_test_deadline();
            if self.auto_advance.get() {
                self.advance_time_by_frame();
            } else {
                std::thread::sleep(Duration::from_millis(10));
                self.run_current();
            }
            checked += 1;
            if checked > checks {
                let condition = description.map_or(String::new(), |d| format!(" ({d})"));
                panic!(
                    "compose-test: ComposeTimeoutException: Condition{condition} still not \
                     satisfied after {} ms",
                    timeout.as_millis()
                );
            }
        }
    }

    fn count_in(&mut self, matcher: &Matcher, use_unmerged_tree: bool) -> usize {
        self.count_nodes_in(matcher, !use_unmerged_tree)
    }

    /// Waits until exactly `count` nodes match `matcher` (in the unmerged tree with
    /// `use_unmerged_tree`), as `waitUntilNodeCount`.
    #[track_caller]
    pub fn wait_until_node_count(
        &mut self,
        matcher: &Matcher,
        count: usize,
        timeout: Duration,
        use_unmerged_tree: bool,
    ) {
        let description = format!("exactly {count} nodes match ({})", matcher.description);
        self.wait_until_with(Some(&description), timeout, |rule| {
            rule.count_in(matcher, use_unmerged_tree) == count
        });
    }

    /// Waits until a node matches, as `waitUntilAtLeastOneExists`.
    #[track_caller]
    pub fn wait_until_at_least_one_exists(
        &mut self,
        matcher: &Matcher,
        timeout: Duration,
        use_unmerged_tree: bool,
    ) {
        let description = format!("at least one node matches ({})", matcher.description);
        self.wait_until_with(Some(&description), timeout, |rule| {
            rule.count_in(matcher, use_unmerged_tree) > 0
        });
    }

    /// Waits until exactly one node matches, as `waitUntilExactlyOneExists`.
    #[track_caller]
    pub fn wait_until_exactly_one_exists(
        &mut self,
        matcher: &Matcher,
        timeout: Duration,
        use_unmerged_tree: bool,
    ) {
        self.wait_until_node_count(matcher, 1, timeout, use_unmerged_tree);
    }

    /// Waits until no node matches, as `waitUntilDoesNotExist`.
    #[track_caller]
    pub fn wait_until_does_not_exist(
        &mut self,
        matcher: &Matcher,
        timeout: Duration,
        use_unmerged_tree: bool,
    ) {
        self.wait_until_node_count(matcher, 0, timeout, use_unmerged_tree);
    }

    // ---- The main clock -----------------------------------------------------------------------

    /// Animations run at `scale` (5: five times as long; 0: they end at once), as a
    /// `MotionDurationScale` in `ComposeUiTestConfig`'s `effectContext`: the thread's scale
    /// (`compose_ui::set_motion_duration_scale`), put back to 1 when the rule goes (D827).
    pub fn set_motion_duration_scale(&mut self, scale: f32) {
        compose_ui::set_motion_duration_scale(scale);
        self.scaled = true;
    }

    /// Whether `wait_for_idle` moves the clock (a frame at a time while one is needed), as
    /// `mainClock.autoAdvance`. Infinite animations run only while it doesn't (D901).
    pub fn set_auto_advance(&mut self, auto_advance: bool) {
        self.auto_advance.set(auto_advance);
    }

    pub fn auto_advance(&self) -> bool {
        self.auto_advance.get()
    }

    /// The test clock's time in milliseconds since the rule started, as `mainClock.currentTime`.
    pub fn current_time_millis(&self) -> u64 {
        (platform::now_nanos() - self.origin_nanos) / 1_000_000
    }

    /// Moves the clock on by `duration` rounded up to whole frames (16 ms), running the frames
    /// and tasks due on the way, as `mainClock.advanceTimeBy(ms)` (D900). Frames keep their own
    /// cadence: one comes a frame duration after something needed it, whatever the calls'
    /// lengths.
    pub fn advance_time_by(&mut self, duration: Duration) {
        let frame = FRAME_DURATION.as_nanos();
        let rounded = duration.as_nanos().div_ceil(frame) * frame;
        self.advance_clock_to(platform::now_nanos() + rounded as u64);
    }

    /// Moves the clock on by exactly `duration`, as `advanceTimeBy(ms, ignoreFrameDuration =
    /// true)`.
    pub fn advance_time_by_ignoring_frame_duration(&mut self, duration: Duration) {
        self.advance_clock_to(platform::now_nanos() + duration.as_nanos() as u64);
    }

    /// Moves the clock one frame on, as `mainClock.advanceTimeByFrame()`.
    pub fn advance_time_by_frame(&mut self) {
        self.advance_clock_to(platform::now_nanos() + FRAME_DURATION.as_nanos() as u64);
    }

    /// Moves the clock a frame at a time until `condition` holds, as
    /// `mainClock.advanceTimeUntil` with its default timeout (1 s of test time).
    #[track_caller]
    pub fn advance_time_until(&mut self, condition: impl FnMut(&mut Self) -> bool) {
        self.advance_time_until_with_timeout(DEFAULT_WAIT_TIMEOUT, condition);
    }

    /// The same, with a `timeout` of test time; panics past it, as `ComposeTimeoutException`.
    #[track_caller]
    pub fn advance_time_until_with_timeout(
        &mut self,
        timeout: Duration,
        mut condition: impl FnMut(&mut Self) -> bool,
    ) {
        let start = platform::now_nanos();
        self.run_current();
        while !condition(self) {
            self.advance_time_by_frame();
            assert!(
                platform::now_nanos() - start <= timeout.as_nanos() as u64,
                "compose-test: ComposeTimeoutException: condition still not satisfied after {} ms",
                timeout.as_millis()
            );
        }
    }

    // ---- Finders ----------------------------------------------------------------------------

    fn interaction(&mut self, selector: Selector, merged: bool) -> SemanticsNodeInteraction<'_> {
        SemanticsNodeInteraction::new(self, selector, merged)
    }

    fn collection(
        &mut self,
        selector: Selector,
        merged: bool,
    ) -> SemanticsNodeInteractionCollection<'_> {
        SemanticsNodeInteractionCollection::new(self, selector, merged)
    }

    /// The node matching `matcher` in the merged semantics tree, as `onNode`.
    pub fn on_node(&mut self, matcher: Matcher) -> SemanticsNodeInteraction<'_> {
        self.interaction(Selector::matching(matcher), true)
    }

    /// The same, in the unmerged tree (`useUnmergedTree = true`).
    pub fn on_node_unmerged(&mut self, matcher: Matcher) -> SemanticsNodeInteraction<'_> {
        self.interaction(Selector::matching(matcher), false)
    }

    /// The node whose text is exactly `text`, as `onNodeWithText`.
    pub fn on_node_with_text(&mut self, text: &str) -> SemanticsNodeInteraction<'_> {
        self.on_node(has_text(text))
    }

    /// As `onNodeWithText(text, useUnmergedTree = true)`.
    pub fn on_node_with_text_unmerged(&mut self, text: &str) -> SemanticsNodeInteraction<'_> {
        self.on_node_unmerged(has_text(text))
    }

    /// The node with content description `description`, as `onNodeWithContentDescription`.
    pub fn on_node_with_content_description(
        &mut self,
        description: &str,
    ) -> SemanticsNodeInteraction<'_> {
        self.on_node(has_content_description(description))
    }

    /// As `onNodeWithContentDescription(description, useUnmergedTree = true)`.
    pub fn on_node_with_content_description_unmerged(
        &mut self,
        description: &str,
    ) -> SemanticsNodeInteraction<'_> {
        self.on_node_unmerged(has_content_description(description))
    }

    /// The node with test tag `tag`, as `onNodeWithTag`.
    pub fn on_node_with_tag(&mut self, tag: &str) -> SemanticsNodeInteraction<'_> {
        self.on_node(has_test_tag(tag))
    }

    /// As `onNodeWithTag(tag, useUnmergedTree = true)`.
    pub fn on_node_with_tag_unmerged(&mut self, tag: &str) -> SemanticsNodeInteraction<'_> {
        self.on_node_unmerged(has_test_tag(tag))
    }

    /// The root, as `onRoot`.
    pub fn on_root(&mut self) -> SemanticsNodeInteraction<'_> {
        self.on_node(is_root())
    }

    /// As `onRoot(useUnmergedTree = true)`.
    pub fn on_root_unmerged(&mut self) -> SemanticsNodeInteraction<'_> {
        self.on_node_unmerged(is_root())
    }

    /// Every node matching `matcher`, in tree order, as `onAllNodes`.
    pub fn on_all_nodes(&mut self, matcher: Matcher) -> SemanticsNodeInteractionCollection<'_> {
        self.collection(Selector::matching(matcher), true)
    }

    /// The same, in the unmerged tree.
    pub fn on_all_nodes_unmerged(
        &mut self,
        matcher: Matcher,
    ) -> SemanticsNodeInteractionCollection<'_> {
        self.collection(Selector::matching(matcher), false)
    }

    /// As `onAllNodesWithText`.
    pub fn on_all_nodes_with_text(&mut self, text: &str) -> SemanticsNodeInteractionCollection<'_> {
        self.on_all_nodes(has_text(text))
    }

    /// As `onAllNodesWithTag`.
    pub fn on_all_nodes_with_tag(&mut self, tag: &str) -> SemanticsNodeInteractionCollection<'_> {
        self.on_all_nodes(has_test_tag(tag))
    }

    /// As `onAllNodesWithContentDescription`.
    pub fn on_all_nodes_with_content_description(
        &mut self,
        description: &str,
    ) -> SemanticsNodeInteractionCollection<'_> {
        self.on_all_nodes(has_content_description(description))
    }

    /// How many nodes match `matcher` in the merged tree, after waiting for idle.
    pub fn count_nodes(&mut self, matcher: &Matcher) -> usize {
        self.count_nodes_in(matcher, true)
    }

    /// How many nodes match `matcher` in the unmerged tree, after waiting for idle.
    pub fn count_nodes_unmerged(&mut self, matcher: &Matcher) -> usize {
        self.count_nodes_in(matcher, false)
    }

    fn count_nodes_in(&mut self, matcher: &Matcher, merged: bool) -> usize {
        self.implicit_wait();
        let tree = self.host.semantics_tree(merged);
        let view = TreeView::new(&tree);
        view.all().filter(|node| matcher.matches_node(*node)).count()
    }

    // ---- Input ------------------------------------------------------------------------------

    /// Sends a key press and release to the focused node.
    pub fn press_key(&mut self, key: Key) {
        self.host.send_key(KeyEvent::down(key));
        self.host.send_key(KeyEvent::up(key));
        self.wait_for_idle();
    }

    /// Types `text` into the focused node.
    pub fn type_text(&mut self, text: &str) {
        self.host.send_text(text);
        self.wait_for_idle();
    }

    /// Body runs per composable so far on this thread: `since` a snapshot, `scopes_named`.
    pub fn recomposition_counts(&self) -> compose_runtime::platform::RecompositionCounts {
        compose_runtime::platform::recomposition_counts()
    }

    // ---- Dumps ------------------------------------------------------------------------------

    /// The node tree with layout, modifiers and semantics.
    pub fn dump_tree(&self) -> String {
        self.host.dump_tree()
    }

    /// The merged semantics tree.
    pub fn dump_semantics(&self) -> String {
        self.host.semantics_tree(true).dump()
    }

    /// The display list of the last frame.
    pub fn dump_display_list(&self) -> String {
        self.host.display_list().dump()
    }

    /// The slot table.
    pub fn dump_slots(&self) -> String {
        self.host.composition().dump()
    }
}

impl Drop for ComposeTestRule {
    /// Takes its infinite-animation policy away, unless another rule's replaced it.
    fn drop(&mut self) {
        if self.scaled {
            compose_ui::set_motion_duration_scale(1.0);
        }
        if platform::infinite_animation_policy()
            .is_some_and(|policy| Rc::ptr_eq(&policy, &self.policy))
        {
            platform::set_infinite_animation_policy(None);
        }
    }
}
