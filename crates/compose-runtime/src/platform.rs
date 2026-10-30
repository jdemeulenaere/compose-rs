//! Host and platform plumbing: what hosts (compose-ui's `UiHost`, compose-test) and other
//! appliers use to drive compositions and the UI thread's executor, and the hooks tests count
//! with. Not for apps, and not covered by semver: these change with the hosts (D260). Apps'
//! tests reach the hooks through `compose::test`.

pub use crate::applier::{Applier, NodeRef};
pub use crate::composer::{Composer, KeptArguments, current};
pub use crate::derived::flush_derived_states;
pub use crate::executor::{
    Delay, WithFrame, advance_clock, cancel_current_task, current_job, dispatch_frame, fire_timers,
    has_frame_awaiters, has_ready_tasks, infinite_animation_policy, launch, next_timer,
    next_timer_deadline, now_nanos, poll_now, run_ready, set_infinite_animation_policy,
    set_manual_clock, set_wake_handler,
};
pub use crate::node_tree::NodeTree;
pub use crate::observe::ReadObserver;
pub use crate::pausable::{PausableContent, PausableContentFor};
pub use crate::remember::remember_across_reuse;
pub use crate::snapshots::Turn;
pub use crate::state::{
    live_state_count, state_observed_read_count, state_reader_capacity, state_reader_count,
    state_records,
};

/// The number of live recompose scopes on this thread. For tests.
pub fn live_scope_count() -> usize {
    crate::runtime::with_runtime(|runtime| runtime.scopes.live_count())
}

/// The number of compactions of state readers run on this thread so far (each linear in the
/// readers of one state). For tests.
pub fn reader_compaction_count() -> u64 {
    crate::runtime::with_runtime(|runtime| runtime.scopes.compactions)
}

/// The number of slots written and side effects recorded by passes on this thread so far: what
/// Compose's change list holds besides node changes (a `remember` computing anew, a keyed
/// `remember` or effect whose keys changed, a `SideEffect`, groups moved or removed). For tests.
pub fn recorded_changes() -> u64 {
    crate::runtime::with_runtime(|runtime| runtime.recorded_changes)
}

/// The number of composable bodies run on this thread so far (restart groups that didn't skip).
/// For tests and frame statistics.
pub fn bodies_run() -> u64 {
    crate::runtime::with_runtime(|runtime| runtime.bodies_run)
}

/// The number of state reads on this thread so far: a caller compares it around code it runs
/// to know whether that code read state (a lazy layout's key function, whose keys it keeps
/// only if they read none).
pub fn state_reads() -> u64 {
    crate::runtime::try_with_runtime(|runtime| runtime.state_reads).unwrap_or(0)
}

/// Starts (or stops) counting body runs per composable on this thread, for
/// [`recomposition_counts`]. For tests and tools; off by default.
pub fn track_recompositions(enabled: bool) {
    crate::runtime::with_runtime(|runtime| {
        if enabled {
            runtime.recomposition_counts.get_or_insert_with(Default::default);
        } else {
            runtime.recomposition_counts = None;
        }
    })
}

/// How many times each composable's body ran since tracking started.
pub fn recomposition_counts() -> RecompositionCounts {
    RecompositionCounts(crate::runtime::with_runtime(|runtime| {
        runtime.recomposition_counts.clone().unwrap_or_default()
    }))
}

/// Body runs per composable, by the type name of its restart closure.
#[derive(Clone, Debug, Default)]
pub struct RecompositionCounts(std::collections::HashMap<&'static str, u64>);

impl RecompositionCounts {
    /// The runs since `before`.
    pub fn since(&self, before: &RecompositionCounts) -> RecompositionCounts {
        RecompositionCounts(
            self.0
                .iter()
                .map(|(name, count)| (*name, count - before.0.get(name).copied().unwrap_or(0)))
                .filter(|(_, count)| *count > 0)
                .collect(),
        )
    }

    /// The runs of composables named `name` (the function's own name, any module).
    pub fn scopes_named(&self, name: &str) -> u64 {
        self.0
            .iter()
            .filter(|(full, _)| composable_name(full) == name)
            .map(|(_, count)| count)
            .sum()
    }

    /// Every composable that ran, by name, sorted.
    pub fn names(&self) -> Vec<(&'static str, u64)> {
        let mut names: Vec<(&'static str, u64)> =
            self.0.iter().map(|(full, count)| (composable_name(full), *count)).collect();
        names.sort();
        names
    }
}

/// `TodoRow` from `app::screen::TodoRow::{{closure}}`.
fn composable_name(full: &'static str) -> &'static str {
    let path = full.split('<').next().unwrap_or(full);
    let path = path.strip_suffix("::{{closure}}").unwrap_or(path);
    path.rsplit("::").next().unwrap_or(path)
}
