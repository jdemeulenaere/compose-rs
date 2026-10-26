//! Emulating an activity's recreation, as Compose's `StateRestorationTester`: the content's
//! saveable state saved, the content disposed, and composed again from what was saved.

use compose_runtime::{
    Callback, CompositionLocalProvider, LOCAL_SAVEABLE_STATE_REGISTRY, MutableState,
    NeverEqualPolicy, OwnedState, SaveableStateRegistry, SavedState, callback, composable,
    mutable_state_of, mutable_state_of_with,
};

use crate::rule::ComposeTestRule;

/// Saves and restores a content's `remember_saveable` state, as Compose's
/// `StateRestorationTester`: give it the content with [`set_content`](Self::set_content),
/// then [`emulate_save_and_restore`](Self::emulate_save_and_restore) disposes the content
/// and composes it again from its saved state, as an activity recreated. What `remember`
/// keeps is lost; what `remember_saveable` keeps comes back.
pub struct StateRestorationTester {
    /// The registry the content saves into and restores from: a fresh one at each restore (a
    /// state: the content's provider reads it, so it's recomposed with the new one).
    registry: OwnedState<SaveableStateRegistry>,
    /// Whether the content is composed (not while it's "recreated").
    emit: OwnedState<bool>,
    content_set: bool,
}

impl Default for StateRestorationTester {
    fn default() -> StateRestorationTester {
        StateRestorationTester::new()
    }
}

impl StateRestorationTester {
    pub fn new() -> StateRestorationTester {
        StateRestorationTester {
            registry: mutable_state_of_with(SaveableStateRegistry::new(None), NeverEqualPolicy),
            emit: mutable_state_of(true),
            content_set: false,
        }
    }

    /// Sets `content` as `rule`'s content, under the tester's saveable state registry, as
    /// `StateRestorationTester.setContent`.
    pub fn set_content(&mut self, rule: &mut ComposeTestRule, content: impl Fn() + 'static) {
        let registry = self.registry.handle();
        let emit = self.emit.handle();
        let content = callback(content);
        self.content_set = true;
        rule.set_content(move || Restorable(emit, registry, content.clone()));
    }

    /// Saves the content's state, disposes the content, then composes it again with the
    /// state restored, waiting for idle after each step, as `emulateSaveAndRestore` (and
    /// junit4's `emulateSavedInstanceStateRestore`). The state goes through its bytes, as
    /// Compose's goes through a `Bundle`.
    #[track_caller]
    pub fn emulate_save_and_restore(&mut self, rule: &mut ComposeTestRule) {
        assert!(self.content_set, "compose-test: set_content should be called first!");
        rule.wait_for_idle();
        let saved = self.registry.with(|registry| registry.perform_save());
        let bytes = saved.to_bytes();
        self.emit.set(false);
        rule.wait_for_idle();
        let restored = SavedState::from_bytes(&bytes).expect("compose-test: saved state decodes");
        self.registry.set(SaveableStateRegistry::new(Some(restored)));
        self.emit.set(true);
        rule.wait_for_idle();
    }
}

/// The content under `registry`, while `emit` says so.
#[composable]
#[allow(non_snake_case)]
fn Restorable(
    emit: MutableState<bool>,
    registry: MutableState<SaveableStateRegistry>,
    content: Callback,
) {
    if emit.get() {
        let registry = registry.with(|registry| registry.clone());
        CompositionLocalProvider(LOCAL_SAVEABLE_STATE_REGISTRY.provides(Some(registry)), move || {
            content()
        });
    }
}
