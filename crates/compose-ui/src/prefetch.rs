//! Work done between frames, as Compose's `PrefetchScheduler`: a lazy list that scrolled asks
//! for the items it will reach next to be composed and measured while the main thread waits for
//! the next frame, so the frame that scrolls them in finds them ready. The host runs the
//! requests (`UiHost::prefetch`) when the platform has idle time, until a deadline.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Instant;

use compose_runtime::{ComposeArg, Composition, CompositionLocal};

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
}

impl PrefetchRequestScope<'_> {
    /// The nanoseconds left until the next frame should start, as `availableTimeNanos`.
    pub fn available_time_nanos(&self) -> u64 {
        self.deadline.saturating_duration_since(Instant::now()).as_nanos() as u64
    }

    /// Recomposes the scopes the request's state writes invalidated, now, outside a frame (as
    /// Compose's precomposition). Returns whether a pass ran.
    pub fn recompose(&mut self) -> bool {
        self.composition.recompose()
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
}

impl PrefetchScheduler {
    /// Runs `request` in the next idle time (and the ones after, while it has work left).
    pub fn schedule_prefetch(&self, request: Rc<dyn PrefetchRequest>) {
        self.requests.borrow_mut().push_back(request);
    }

    pub(crate) fn has_requests(&self) -> bool {
        !self.requests.borrow().is_empty()
    }

    /// Runs the requests in order until `deadline`: one with work left keeps its place and ends
    /// this idle time, as Compose's. Returns whether requests are left.
    pub(crate) fn run(&self, composition: &Composition, deadline: Instant) -> bool {
        loop {
            let Some(request) = self.requests.borrow().front().cloned() else { return false };
            let mut scope = PrefetchRequestScope { deadline, composition };
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
