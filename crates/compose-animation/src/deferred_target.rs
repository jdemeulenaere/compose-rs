//! `DeferredTargetAnimation`, as Compose's: an animation whose target is set where it's known
//! (while measuring, say), animating to each new one.

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::{CoroutineScope, OwnedState, mutable_state_of};

use crate::animatable::Animatable;
use crate::spec::AnimationSpec;
use crate::vector::Animate;

/// An animation to targets given as they become known, as Compose's `DeferredTargetAnimation`:
/// the first target is its value at once; each later one is animated to, along the spec given
/// with it, by a task of the scope given with it.
pub struct DeferredTargetAnimation<T: Animate> {
    inner: Rc<Inner<T>>,
}

struct Inner<T: Animate> {
    pending_target: OwnedState<Option<T>>,
    animatable: RefCell<Option<Animatable<T>>>,
}

impl<T: Animate> Clone for DeferredTargetAnimation<T> {
    fn clone(&self) -> Self {
        DeferredTargetAnimation { inner: self.inner.clone() }
    }
}

impl<T: Animate> Default for DeferredTargetAnimation<T> {
    #[track_caller]
    fn default() -> Self {
        DeferredTargetAnimation::new()
    }
}

impl<T: Animate> DeferredTargetAnimation<T> {
    /// An animation with no target yet, as `DeferredTargetAnimation(vectorConverter)`.
    #[track_caller]
    pub fn new() -> DeferredTargetAnimation<T> {
        DeferredTargetAnimation {
            inner: Rc::new(Inner {
                pending_target: mutable_state_of(None),
                animatable: RefCell::new(None),
            }),
        }
    }

    /// The last target given, as `pendingTarget` (a state read).
    pub fn pending_target(&self) -> Option<T> {
        self.inner.pending_target.get()
    }

    /// The value, once a target was given, as `value` (a state read).
    pub fn value(&self) -> Option<T> {
        self.inner.animatable.borrow().as_ref().map(Animatable::value)
    }

    /// Whether it's at its last target and not animating, as `isIdle`.
    pub fn is_idle(&self) -> bool {
        let animatable = self.inner.animatable.borrow();
        let target = animatable.as_ref().map(Animatable::target_value);
        self.pending_target() == target && !animatable.as_ref().is_some_and(Animatable::is_running)
    }

    /// Sets the target, animated to along `animation_spec` by a task of `scope` unless it's the
    /// target already (the first one is the value at once), as `updateTarget`: the value now.
    pub fn update_target(
        &self,
        target: T,
        scope: CoroutineScope,
        animation_spec: impl Into<AnimationSpec>,
    ) -> T {
        self.inner.pending_target.set(Some(target.clone()));
        let animatable = self
            .inner
            .animatable
            .borrow_mut()
            .get_or_insert_with(|| Animatable::new(target.clone()))
            .clone();
        let (pending, spec) = (self.inner.pending_target.handle(), animation_spec.into());
        let running = animatable.clone();
        scope.launch(async move {
            if Some(running.target_value()) != pending.get() {
                running.animate_to(target, spec).await;
            }
        });
        animatable.value()
    }
}
