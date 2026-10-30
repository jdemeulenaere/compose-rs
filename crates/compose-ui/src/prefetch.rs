//! Work done between frames, as Compose's `PrefetchScheduler`: a lazy list that scrolled asks
//! for the items it will reach next to be composed and measured while the main thread waits for
//! the next frame, so the frame that scrolls them in finds them ready. The host runs the
//! requests (`UiHost::prefetch`) when the platform has idle time, until a deadline.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

use compose_runtime::{ComposeArg, Composition, CompositionLocal};

thread_local! {
    /// The time a `PrefetchClock` holds, instead of the machine's.
    static HELD: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// The time prefetching goes by, deadlines and the steps it times alike: the machine's, unless a
/// test holds a [`PrefetchClock`].
#[doc(hidden)]
pub fn prefetch_now() -> Instant {
    HELD.get().unwrap_or_else(Instant::now)
}

/// A test's clock for prefetching on this thread, until it's dropped: it stands still but for what
/// [`advance`](PrefetchClock::advance) says, so a step takes what the test says it takes and an
/// idle time never runs out on a loaded machine (D1457).
#[doc(hidden)]
pub struct PrefetchClock;

impl PrefetchClock {
    pub fn hold() -> PrefetchClock {
        HELD.set(Some(Instant::now()));
        PrefetchClock
    }

    /// Whether a test holds the clock on this thread.
    pub fn held() -> bool {
        HELD.get().is_some()
    }

    pub fn advance(by: Duration) {
        HELD.set(HELD.get().map(|now| now + by));
    }
}

impl Drop for PrefetchClock {
    fn drop(&mut self) {
        HELD.set(None);
    }
}

use crate::constraints::Constraints;
use crate::tree::{NodeId, UiTree};
use crate::unit::IntSize;

/// Work to do between frames, as Compose's `PrefetchRequest`.
pub trait PrefetchRequest {
    /// Does the work, or as much of it as the time left allows; returns whether some is left,
    /// to do in the next idle time.
    fn execute(&self, scope: &mut PrefetchRequestScope<'_>) -> bool;
}

/// What a request works with: the time left, and the composition and tree to compose and
/// measure in, as Compose's `PrefetchRequestScope`.
pub struct PrefetchRequestScope<'a> {
    pub(crate) deadline: Instant,
    pub(crate) composition: &'a Composition,
    pub(crate) idle_time: u64,
}

impl PrefetchRequestScope<'_> {
    /// The nanoseconds left until the next frame should start, as `availableTimeNanos`.
    pub fn available_time_nanos(&self) -> u64 {
        self.deadline.saturating_duration_since(prefetch_now()).as_nanos() as u64
    }

    /// Which idle time this is: a number the scheduler counts up at each run, so a request can
    /// tell its first step in an idle time from the next ones.
    pub fn idle_time(&self) -> u64 {
        self.idle_time
    }

    /// When the next frame should start: what `available_time_nanos` counts down to.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Recomposes the scopes the request's state writes invalidated, now, outside a frame (as
    /// Compose's precomposition). Returns whether a pass ran.
    pub fn recompose(&mut self) -> bool {
        self.composition.recompose()
    }

    /// A paused composition of the scopes the request's state writes invalidated, as Compose's
    /// paused precomposition (`createPausedPrecomposition`): resumed until complete (in this
    /// idle time and the next ones), then applied; the `PausableContent` it inserts or reuses
    /// pauses where its callback says.
    pub fn recompose_pausable(&mut self) -> compose_runtime::PausedComposition {
        self.composition.recompose_pausable()
    }

    /// As [`recompose_pausable`](Self::recompose_pausable), pausing only the
    /// `PausableContentFor` of `target` (the request's item's content).
    pub fn recompose_pausable_for(&mut self, target: u64) -> compose_runtime::PausedComposition {
        self.composition.recompose_pausable_for(target)
    }

    /// Measures `parent`'s child at `index` with `constraints` before the layout that places it,
    /// as Compose's `premeasure`: that layout finds it measured. Returns its size, or `None`
    /// without such a child.
    pub fn premeasure(
        &mut self,
        parent: NodeId,
        index: usize,
        constraints: Constraints,
    ) -> Option<IntSize> {
        self.composition
            .with_applier(|tree: &mut UiTree| tree.premeasure(parent, index, constraints))
    }
}

/// Where requests wait for idle time, as Compose's `PrefetchScheduler`: the host's, given to
/// content as `LOCAL_PREFETCH_SCHEDULER`.
#[derive(Clone, Default)]
pub struct PrefetchScheduler {
    requests: Rc<RefCell<VecDeque<Rc<dyn PrefetchRequest>>>>,
    /// The idle times run so far.
    runs: Rc<std::cell::Cell<u64>>,
}

impl PrefetchScheduler {
    /// Runs `request` in the next idle time (and the ones after, while it has work left).
    pub fn schedule_prefetch(&self, request: Rc<dyn PrefetchRequest>) {
        self.requests.borrow_mut().push_back(request);
    }

    /// Runs `request` first in the next idle time, before the others: work that a frame's
    /// layout leaves which Compose's does in the layout itself (a lazy list's deactivations).
    pub fn schedule_prefetch_first(&self, request: Rc<dyn PrefetchRequest>) {
        self.requests.borrow_mut().push_front(request);
    }

    pub(crate) fn has_requests(&self) -> bool {
        !self.requests.borrow().is_empty()
    }

    /// Runs the requests in order until `deadline`: one with work left keeps its place and ends
    /// this idle time, as Compose's (`AndroidPrefetchScheduler.run`: a request runs while time
    /// is left, and is taken off the queue once it has no work left). With `unbounded_first`
    /// (no frame drawn for two intervals: Compose's `isFrameIdle`), the first request runs
    /// without a deadline. Returns whether requests are left.
    pub(crate) fn run(
        &self,
        composition: &Composition,
        deadline: Instant,
        unbounded_first: bool,
    ) -> bool {
        let idle_time = self.runs.get() + 1;
        self.runs.set(idle_time);
        let mut unbounded = unbounded_first;
        loop {
            let Some(request) = self.requests.borrow().front().cloned() else { return false };
            let deadline =
                if unbounded { prefetch_now() + Duration::from_secs(3600) } else { deadline };
            unbounded = false;
            let mut scope = PrefetchRequestScope { deadline, composition, idle_time };
            if scope.available_time_nanos() == 0 || request.execute(&mut scope) {
                return true;
            }
            self.requests.borrow_mut().pop_front();
        }
    }
}

impl ComposeArg for PrefetchScheduler {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.requests, &other.requests)
    }
}

/// The host's prefetch scheduler (`None` outside a host: nothing is prefetched).
pub static LOCAL_PREFETCH_SCHEDULER: CompositionLocal<Option<PrefetchScheduler>> =
    CompositionLocal::new("LocalPrefetchScheduler", || None);
