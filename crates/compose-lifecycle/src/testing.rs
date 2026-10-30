//! As AndroidX's `lifecycle-runtime-testing`.

use crate::lifecycle::{
    Lifecycle, LifecycleEvent, LifecycleOwner, LifecycleRegistry, LifecycleState,
};

/// A lifecycle a test moves, as AndroidX's `TestLifecycleOwner`.
#[derive(Clone)]
pub struct TestLifecycleOwner {
    registry: LifecycleRegistry,
}

impl Default for TestLifecycleOwner {
    /// At `Started`, as `TestLifecycleOwner()`.
    fn default() -> TestLifecycleOwner {
        TestLifecycleOwner::new(LifecycleState::Started)
    }
}

impl TestLifecycleOwner {
    /// At `initial_state`, as `TestLifecycleOwner(initialState)`.
    pub fn new(initial_state: LifecycleState) -> TestLifecycleOwner {
        let registry = LifecycleRegistry::new();
        registry.set_current_state(initial_state);
        TestLifecycleOwner { registry }
    }

    /// As `handleLifecycleEvent`.
    pub fn handle_lifecycle_event(&self, event: LifecycleEvent) {
        self.registry.handle_lifecycle_event(event);
    }

    /// As `currentState`.
    pub fn current_state(&self) -> LifecycleState {
        self.registry.current_state()
    }

    /// As `currentState = state`.
    pub fn set_current_state(&self, state: LifecycleState) {
        self.registry.set_current_state(state);
    }

    /// As `observerCount`.
    pub fn observer_count(&self) -> usize {
        self.registry.observer_count()
    }
}

impl LifecycleOwner for TestLifecycleOwner {
    fn lifecycle(&self) -> Lifecycle {
        self.registry.lifecycle()
    }
}
