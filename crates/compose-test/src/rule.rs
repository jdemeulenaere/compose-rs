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

/// How long `wait_for_idle` lets the test clock run (animations) before deciding the UI never
/// idles.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

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
    /// Whether finders wait for idle first, as Compose's implicit wait
    /// (`run_without_implicit_wait` turns it off for its block).
    implicit_wait: bool,
    /// The bounds assertions' tolerance given none, as `ComposeUiTestConfig`'s
    /// `boundsAssertionTolerance`.
    bounds_tolerance: Dp,
    /// The injected pointers and keys, kept across `perform_*_input` blocks, as Compose's
    /// `InputDispatcher` state.
    pub(crate) input: InputState,
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
            implicit_wait: true,
            bounds_tolerance: DEFAULT_BOUNDS_TOLERANCE,
            input: InputState::default(),
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
        let mut frames = 0;
        loop {
            self.run_current();
            if self.next_frame.is_none() || !self.auto_advance.get() {
                return;
            }
            let elapsed = Duration::from_nanos(platform::now_nanos() - start);
            assert!(
                elapsed <= IDLE_TIMEOUT && frames < 100_000,
                "compose-test: the UI never became idle ({frames} frames, {elapsed:?} of test \
                 time): an animation or a recomposition that never ends?"
            );
            self.advance_clock_to(platform::now_nanos() + FRAME_DURATION.as_nanos() as u64);
            frames += 1;
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
        if platform::infinite_animation_policy()
            .is_some_and(|policy| Rc::ptr_eq(&policy, &self.policy))
        {
            platform::set_infinite_animation_policy(None);
        }
    }
}
