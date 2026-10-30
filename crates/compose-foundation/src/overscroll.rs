//! What a scroll does past its bounds, as Compose's `Overscroll.kt`: an [`OverscrollEffect`] takes a
//! scrollable's deltas and velocities around the scrolling, and draws by [`Modifier::overscroll`]
//! (D1464).

use std::rc::Rc;

use compose_runtime::{CompositionLocal, composable};
use compose_ui::{Modifier, NestedScrollSource, Offset, Velocity};

use crate::gestures::LocalBoxFuture;

/// The fling an [`OverscrollEffect`] is given: it flings with the velocity it's given, and returns
/// the velocity consumed.
pub type PerformFling = Rc<dyn Fn(Velocity) -> LocalBoxFuture<'static, Velocity>>;

/// A scroll's effect past its bounds, as Compose's `OverscrollEffect`.
///
/// Compose's `node` is [`modifier`](Self::modifier) here: a node of compose-rs belongs to one chain
/// (its delegates are owned), so the effect gives what draws it to the place that asks. A
/// `Modifier::overscroll` with the same effect at two places draws it twice, where Compose attaches
/// its one node once.
pub trait OverscrollEffect {
    /// A scroll of `delta` (the pointer's way) from `source`: the effect may take a part first,
    /// calls `perform_scroll` with the rest (it returns what the scrollable and the nested scroll
    /// used) and takes what's left. Returns what was consumed, its own part included.
    fn apply_to_scroll(
        &self,
        delta: Offset,
        source: NestedScrollSource,
        perform_scroll: &mut dyn FnMut(Offset) -> Offset,
    ) -> Offset;

    /// A fling at `velocity` (the pointer's way): as [`apply_to_scroll`](Self::apply_to_scroll), the
    /// effect calls `perform_fling` with what it doesn't take.
    fn apply_to_fling<'a>(
        &'a self,
        velocity: Velocity,
        perform_fling: PerformFling,
    ) -> LocalBoxFuture<'a, ()>;

    /// Whether the effect is showing: a press then takes it, without a slop.
    fn is_in_progress(&self) -> bool;

    /// What draws it, as `node`: nothing by default.
    fn modifier(&self) -> Modifier {
        Modifier
    }
}

/// An effect without one of its sides, as `withoutVisualEffect` and `withoutEventHandling`.
struct WrappedOverscrollEffect {
    draws: bool,
    handles_events: bool,
    inner: Rc<dyn OverscrollEffect>,
}

impl OverscrollEffect for WrappedOverscrollEffect {
    fn apply_to_scroll(
        &self,
        delta: Offset,
        source: NestedScrollSource,
        perform_scroll: &mut dyn FnMut(Offset) -> Offset,
    ) -> Offset {
        if self.handles_events {
            self.inner.apply_to_scroll(delta, source, perform_scroll)
        } else {
            perform_scroll(delta)
        }
    }

    fn apply_to_fling<'a>(
        &'a self,
        velocity: Velocity,
        perform_fling: PerformFling,
    ) -> LocalBoxFuture<'a, ()> {
        if self.handles_events {
            self.inner.apply_to_fling(velocity, perform_fling)
        } else {
            Box::pin(async move {
                perform_fling(velocity).await;
            })
        }
    }

    fn is_in_progress(&self) -> bool {
        self.inner.is_in_progress()
    }

    fn modifier(&self) -> Modifier {
        if self.draws { self.inner.modifier() } else { Modifier }
    }
}

/// `effect` handling the events only: for a place drawing it another way, as `withoutVisualEffect`.
pub fn without_visual_effect(effect: Rc<dyn OverscrollEffect>) -> Rc<dyn OverscrollEffect> {
    Rc::new(WrappedOverscrollEffect { draws: false, handles_events: true, inner: effect })
}

/// `effect` drawing only: for a place handling its events another way, as `withoutEventHandling`.
pub fn without_event_handling(effect: Rc<dyn OverscrollEffect>) -> Rc<dyn OverscrollEffect> {
    Rc::new(WrappedOverscrollEffect { draws: true, handles_events: false, inner: effect })
}

/// Makes the effects of a place, as Compose's `OverscrollFactory`.
pub trait OverscrollFactory {
    fn create_overscroll_effect(&self) -> Rc<dyn OverscrollEffect>;
}

/// The factory of [`remember_overscroll_effect`], as `LocalOverscrollFactory`. None by default:
/// Android's stretch and glow, which Compose's default gives, are yet to come (D1464).
pub static LOCAL_OVERSCROLL_FACTORY: CompositionLocal<Option<Rc<dyn OverscrollFactory>>> =
    CompositionLocal::new("LOCAL_OVERSCROLL_FACTORY", || None);

/// An effect of [`LOCAL_OVERSCROLL_FACTORY`]'s, the same until the factory changes, as
/// `rememberOverscrollEffect`.
#[composable(non_restartable)]
pub fn remember_overscroll_effect() -> Option<Rc<dyn OverscrollEffect>> {
    let factory = LOCAL_OVERSCROLL_FACTORY.current();
    // Remembered with or without a factory: a slot's place doesn't depend on it.
    let slot = compose_runtime::remember(|| {
        Rc::new(std::cell::RefCell::new(
            None::<(Rc<dyn OverscrollFactory>, Rc<dyn OverscrollEffect>)>,
        ))
    });
    let mut slot = slot.borrow_mut();
    let factory = factory?;
    // By identity, where Compose's `remember(overscrollFactory)` uses `equals`: a factory is an
    // `Rc` made once where it's provided (D1465), so identity is what an app means by the same one.
    match &*slot {
        Some((made_by, effect)) if Rc::ptr_eq(made_by, &factory) => Some(effect.clone()),
        _ => {
            let effect = factory.create_overscroll_effect();
            *slot = Some((factory, effect.clone()));
            Some(effect)
        }
    }
}

/// Draws an effect at the place it's applied.
pub trait OverscrollModifier {
    /// Draws `effect`, as `Modifier.overscroll`; `None` is nothing.
    fn overscroll(self, effect: Option<Rc<dyn OverscrollEffect>>) -> Modifier;
}

impl OverscrollModifier for Modifier {
    fn overscroll(self, effect: Option<Rc<dyn OverscrollEffect>>) -> Modifier {
        match effect {
            Some(effect) => self.then(effect.modifier()),
            None => self,
        }
    }
}

/// Whether two optional effects are the same one, the way elements compare theirs.
pub(crate) fn same_overscroll(
    a: &Option<Rc<dyn OverscrollEffect>>,
    b: &Option<Rc<dyn OverscrollEffect>>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
