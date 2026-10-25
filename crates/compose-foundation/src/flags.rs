//! Switches for foundation behaviours, as Compose's `ComposeFoundationFlags`.

use std::cell::Cell;

thread_local! {
    static PAUSABLE_COMPOSITION_IN_PREFETCH: Cell<bool> = const { Cell::new(true) };
}

/// Switches for foundation behaviours, as Compose's `ComposeFoundationFlags` (this thread's: a
/// UI thread's compositions).
pub struct ComposeFoundationFlags;

impl ComposeFoundationFlags {
    /// Whether lazy layouts' prefetch composes an item with a paused composition, which pauses
    /// when the idle time runs out and resumes after the next frame (Compose's
    /// `isPausableCompositionInPrefetchEnabled`, true by default). Off, a request composes its
    /// item whole, once the time left fits the whole composition's average (D590).
    pub fn is_pausable_composition_in_prefetch_enabled() -> bool {
        PAUSABLE_COMPOSITION_IN_PREFETCH.with(Cell::get)
    }

    /// Sets [`is_pausable_composition_in_prefetch_enabled`] for this thread, from the next
    /// layout on.
    ///
    /// [`is_pausable_composition_in_prefetch_enabled`]:
    ///     ComposeFoundationFlags::is_pausable_composition_in_prefetch_enabled
    pub fn set_pausable_composition_in_prefetch_enabled(enabled: bool) {
        PAUSABLE_COMPOSITION_IN_PREFETCH.with(|flag| flag.set(enabled));
    }
}
