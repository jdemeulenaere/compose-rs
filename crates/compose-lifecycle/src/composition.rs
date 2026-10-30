//! Lifecycles in composition, as AndroidX's `lifecycle-runtime-compose`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::{
    ComposeArg, CompositionLocal, DisposableEffect, LaunchedEffect, composable, remember,
    remember_keyed,
};

use crate::lifecycle::{
    Lifecycle, LifecycleEvent, LifecycleObserver, LifecycleOwner, LifecycleRegistry, LifecycleState,
};

/// The lifecycle of what's composing, as AndroidX's `LocalLifecycleOwner`: the host's unless
/// something provides another. Outside any host, it panics, as AndroidX's throws.
pub static LOCAL_LIFECYCLE_OWNER: CompositionLocal<Lifecycle> =
    CompositionLocal::new("LocalLifecycleOwner", || {
        panic!("CompositionLocal LocalLifecycleOwner not present")
    });

/// Runs `on_event` on each `event` of the lifecycle, as AndroidX's `LifecycleEventEffect`. The
/// latest `on_event` runs. Panics for `OnDestroy`, which comes after the composition is disposed.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn LifecycleEventEffect(
    event: LifecycleEvent,
    #[default(crate::LOCAL_LIFECYCLE_OWNER.current())] lifecycle_owner: Lifecycle,
    on_event: impl Fn() + 'static,
) {
    assert!(
        event != LifecycleEvent::OnDestroy,
        "LifecycleEventEffect cannot be used to listen for Lifecycle.Event.ON_DESTROY, since \
         Compose disposes of the composition before ON_DESTROY observers are invoked."
    );
    let latest = remember(|| Rc::new(RefCell::new(None::<compose_runtime::Callback>)));
    *latest.borrow_mut() = Some(on_event);
    let lifecycle = lifecycle_owner.clone();
    DisposableEffect(lifecycle_owner, move |scope| {
        let observer: Rc<dyn LifecycleObserver> =
            Rc::new(move |_: &Lifecycle, happened: LifecycleEvent| {
                if happened == event {
                    let on_event = latest.borrow().clone();
                    if let Some(on_event) = on_event {
                        on_event();
                    }
                }
            });
        lifecycle.add_observer(observer.clone());
        scope.on_dispose(move || lifecycle.remove_observer(&observer))
    });
}

/// What [`LifecycleStartEffect`]'s block gives, what runs on stop or disposal, as AndroidX's
/// `LifecycleStopOrDisposeEffectResult`.
pub struct LifecycleStopOrDisposeEffectResult {
    effect: Box<dyn FnOnce(&Lifecycle)>,
}

/// What [`LifecycleStartEffect`]'s block is given, as AndroidX's `LifecycleStartStopEffectScope`.
pub struct LifecycleStartStopEffectScope {
    lifecycle: Lifecycle,
}

impl LifecycleStartStopEffectScope {
    /// The lifecycle of the effect, as `lifecycle`.
    pub fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    /// What to run when the lifecycle stops or the effect is disposed, as `onStopOrDispose`.
    pub fn on_stop_or_dispose(
        &self,
        effect: impl FnOnce(&Lifecycle) + 'static,
    ) -> LifecycleStopOrDisposeEffectResult {
        LifecycleStopOrDisposeEffectResult { effect: Box::new(effect) }
    }
}

/// What [`LifecycleResumeEffect`]'s block gives, as AndroidX's
/// `LifecyclePauseOrDisposeEffectResult`.
pub struct LifecyclePauseOrDisposeEffectResult {
    effect: Box<dyn FnOnce(&Lifecycle)>,
}

/// What [`LifecycleResumeEffect`]'s block is given, as AndroidX's
/// `LifecycleResumePauseEffectScope`.
pub struct LifecycleResumePauseEffectScope {
    lifecycle: Lifecycle,
}

impl LifecycleResumePauseEffectScope {
    /// The lifecycle of the effect, as `lifecycle`.
    pub fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    /// What to run when the lifecycle pauses or the effect is disposed, as `onPauseOrDispose`.
    pub fn on_pause_or_dispose(
        &self,
        effect: impl FnOnce(&Lifecycle) + 'static,
    ) -> LifecyclePauseOrDisposeEffectResult {
        LifecyclePauseOrDisposeEffectResult { effect: Box::new(effect) }
    }
}

/// Starting and stopping effects' shared work: `begin` on `up`, what it gave on `down` or disposal.
fn paired_effect<K: ComposeArg>(
    key: K,
    lifecycle: Lifecycle,
    up: LifecycleEvent,
    down: LifecycleEvent,
    begin: impl Fn(&Lifecycle) -> Box<dyn FnOnce(&Lifecycle)> + 'static,
) {
    let observed = lifecycle.clone();
    DisposableEffect((key, lifecycle), move |scope| {
        let pending = Rc::new(Cell::new(None::<Box<dyn FnOnce(&Lifecycle)>>));
        let held = pending.clone();
        let observer: Rc<dyn LifecycleObserver> =
            Rc::new(move |source: &Lifecycle, event: LifecycleEvent| {
                if event == up {
                    held.set(Some(begin(source)));
                } else if event == down
                    && let Some(end) = held.take()
                {
                    end(source);
                }
            });
        observed.add_observer(observer.clone());
        scope.on_dispose(move || {
            observed.remove_observer(&observer);
            if let Some(end) = pending.take() {
                end(&observed);
            }
        })
    });
}

/// Runs `effects` on each start of the lifecycle and what it gives on each stop or on disposal, as
/// AndroidX's `LifecycleStartEffect(keys)`: a new `key` (a tuple for several) or lifecycle ends and
/// starts it again.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn LifecycleStartEffect<K: ComposeArg + Clone + 'static>(
    key: K,
    #[default(crate::LOCAL_LIFECYCLE_OWNER.current())] lifecycle_owner: Lifecycle,
    effects: impl Fn(LifecycleStartStopEffectScope) -> LifecycleStopOrDisposeEffectResult + 'static,
) {
    paired_effect(
        key,
        lifecycle_owner,
        LifecycleEvent::OnStart,
        LifecycleEvent::OnStop,
        move |lifecycle| {
            effects(LifecycleStartStopEffectScope { lifecycle: lifecycle.clone() }).effect
        },
    );
}

/// Runs `effects` on each resume of the lifecycle and what it gives on each pause or on disposal,
/// as AndroidX's `LifecycleResumeEffect(keys)`.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn LifecycleResumeEffect<K: ComposeArg + Clone + 'static>(
    key: K,
    #[default(crate::LOCAL_LIFECYCLE_OWNER.current())] lifecycle_owner: Lifecycle,
    effects: impl Fn(LifecycleResumePauseEffectScope) -> LifecyclePauseOrDisposeEffectResult + 'static,
) {
    paired_effect(
        key,
        lifecycle_owner,
        LifecycleEvent::OnResume,
        LifecycleEvent::OnPause,
        move |lifecycle| {
            effects(LifecycleResumePauseEffectScope { lifecycle: lifecycle.clone() }).effect
        },
    );
}

fn drop_unless_state_is_at_least(
    state: LifecycleState,
    lifecycle_owner: Lifecycle,
    block: impl Fn() + 'static,
) -> impl Fn() + 'static {
    move || {
        if lifecycle_owner.current_state().is_at_least(state) {
            block();
        }
    }
}

/// `block`, doing nothing unless the lifecycle is at least started when it's called, as AndroidX's
/// `dropUnlessStarted`: a click on content leaving, say.
pub fn drop_unless_started(
    lifecycle_owner: &impl LifecycleOwner,
    block: impl Fn() + 'static,
) -> impl Fn() + 'static {
    drop_unless_state_is_at_least(LifecycleState::Started, lifecycle_owner.lifecycle(), block)
}

/// The same unless resumed, as AndroidX's `dropUnlessResumed`.
pub fn drop_unless_resumed(
    lifecycle_owner: &impl LifecycleOwner,
    block: impl Fn() + 'static,
) -> impl Fn() + 'static {
    drop_unless_state_is_at_least(LifecycleState::Resumed, lifecycle_owner.lifecycle(), block)
}

/// A lifecycle of this part of the composition, as AndroidX's `ComposeLifecycleOwner`.
struct ComposeLifecycle {
    registry: LifecycleRegistry,
    parent_state: Cell<LifecycleState>,
    max_state: Cell<LifecycleState>,
}

impl ComposeLifecycle {
    fn handle_lifecycle_event(&self, event: LifecycleEvent) {
        self.parent_state.set(event.target_state());
        self.update();
    }

    fn update(&self) {
        let target = self.parent_state.get().min(self.max_state.get());
        if self.registry.current_state() == LifecycleState::Initialized
            && target == LifecycleState::Destroyed
        {
            return;
        }
        self.registry.set_current_state(target);
    }
}

/// A lifecycle following `parent` up to `max_lifecycle`, destroyed when it leaves the composition,
/// as AndroidX's `rememberLifecycleOwner`. With no parent, it goes to `max_lifecycle` by itself.
#[composable]
pub fn remember_lifecycle_owner(
    #[default(crate::LifecycleState::Resumed)] max_lifecycle: LifecycleState,
    #[default(Some(crate::LOCAL_LIFECYCLE_OWNER.current()))] parent: Option<Lifecycle>,
) -> Lifecycle {
    let local = remember_keyed(parent.clone(), || {
        Rc::new(ComposeLifecycle {
            registry: LifecycleRegistry::new(),
            parent_state: Cell::new(LifecycleState::Initialized),
            max_state: Cell::new(LifecycleState::Initialized),
        })
    });
    let lifecycle = local.registry.lifecycle();
    let followed = local.clone();
    DisposableEffect((lifecycle.clone(), parent.clone()), move |scope| {
        let following = followed.clone();
        let observer: Rc<dyn LifecycleObserver> =
            Rc::new(move |_: &Lifecycle, event: LifecycleEvent| {
                following.handle_lifecycle_event(event);
            });
        if let Some(parent) = &parent {
            parent.add_observer(observer.clone());
        } else {
            followed.handle_lifecycle_event(LifecycleEvent::OnResume);
        }
        scope.on_dispose(move || {
            if let Some(parent) = &parent {
                parent.remove_observer(&observer);
            }
            followed.handle_lifecycle_event(LifecycleEvent::OnDestroy);
        })
    });
    let capped = local.clone();
    LaunchedEffect((lifecycle.clone(), max_lifecycle), async move {
        capped.max_state.set(max_lifecycle);
        capped.update();
    });
    lifecycle
}
