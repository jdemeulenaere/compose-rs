//! The scale of animations' durations, as Compose's `MotionDurationScale` (D827): Android's
//! animator duration scale, or a test's. Compose's is a coroutine-context element; compose-rs's
//! tasks have no context, so a thread has a scale (a host's, or a test rule's: Compose's
//! composition `effectContext`), and `with_motion_duration_scale` gives a task's future one of
//! its own while it is polled (Compose's `withContext(MotionDurationScale)`). Animations read it
//! as they wait for a frame: a scale of 5 makes them five times as long, 0 ends them at once.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

thread_local! {
    /// The thread's scale.
    static DEFAULT_SCALE: Cell<f32> = const { Cell::new(1.0) };
    /// The scale of the future being polled under `with_motion_duration_scale`, if any.
    static SCOPED_SCALE: Cell<Option<f32>> = const { Cell::new(None) };
    /// Where the thread's scale is read from at its first use, a host's platform setting.
    static SOURCE: Cell<Option<fn() -> f32>> = const { Cell::new(None) };
    /// What waits for the thread's scale to be above 0, by the id of its wait: each has one waker,
    /// the last, and takes it away when it's dropped (a task cancelled).
    static WAITING: RefCell<Vec<(u64, Waker)>> = const { RefCell::new(Vec::new()) };
    static NEXT_WAIT_ID: Cell<u64> = const { Cell::new(0) };
}

/// The scale animations run at here, as Compose's `coroutineContext.durationScale`: the
/// innermost [`with_motion_duration_scale`] around the code polled, else the thread's (1 unless
/// [`set_motion_duration_scale`] set another).
pub fn motion_duration_scale() -> f32 {
    if let Some(scoped) = SCOPED_SCALE.with(Cell::get) {
        return scoped;
    }
    if let Some(source) = SOURCE.with(Cell::get) {
        set_motion_duration_scale(source());
    }
    DEFAULT_SCALE.with(Cell::get)
}

/// Has the thread's scale read from `source` the first time it's used, as Compose's
/// `MotionDurationScaleImpl` starts observing the setting at the first read of `scaleFactor`: a
/// screen with no animation doesn't pay for the platform's calls (D839). What a set does at once
/// with its value, the first read does with `source`'s; a set before it takes `source`'s place.
pub fn set_motion_duration_scale_source(source: fn() -> f32) {
    SOURCE.with(|kept| kept.set(Some(source)));
}

/// Sets the thread's scale, as a host's `MotionDurationScale` (Android's animator scale) or a
/// test's composition `effectContext`: animations running read it at their next frame, and
/// those waiting for a scale above 0 (an infinite transition at 0) go on. Panics if it is
/// negative, as Compose's.
pub fn set_motion_duration_scale(scale: f32) {
    assert!(scale >= 0.0 || scale.is_nan(), "negative scale factor");
    SOURCE.with(|source| source.set(None));
    DEFAULT_SCALE.with(|default| default.set(scale));
    if scale > 0.0 {
        let waiting = WAITING.with(|waiting| std::mem::take(&mut *waiting.borrow_mut()));
        for (_, waker) in waiting {
            waker.wake();
        }
    }
}

/// `future` with a scale of its own while it is polled, as Compose's
/// `withContext(MotionDurationScale)`. Panics at once if `scale` is negative.
pub fn with_motion_duration_scale<F: Future>(
    scale: f32,
    future: F,
) -> impl Future<Output = F::Output> {
    assert!(scale >= 0.0 || scale.is_nan(), "negative scale factor");
    let mut future = Box::pin(future);
    std::future::poll_fn(move |context: &mut Context<'_>| {
        let previous = SCOPED_SCALE.with(|scoped| scoped.replace(Some(scale)));
        struct Restore(Option<f32>);
        impl Drop for Restore {
            fn drop(&mut self) {
                SCOPED_SCALE.with(|scoped| scoped.set(self.0));
            }
        }
        let _restore = Restore(previous);
        Pin::new(&mut future).poll(context)
    })
}

/// A wait for a scale above 0: its entry in `WAITING` goes with it.
struct PositiveScale {
    id: Option<u64>,
}

impl PositiveScale {
    fn leave(&mut self) {
        if let Some(id) = self.id.take() {
            // `try_with`: dropped as the thread ends, the list may be gone.
            let _ = WAITING.try_with(|waiting| {
                waiting.borrow_mut().retain(|(waiter, _)| *waiter != id);
            });
        }
    }
}

impl Future for PositiveScale {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        if motion_duration_scale() > 0.0 {
            self.leave();
            return Poll::Ready(());
        }
        let id = *self.id.get_or_insert_with(|| {
            NEXT_WAIT_ID.with(|next| next.replace(next.get().wrapping_add(1)))
        });
        WAITING.with(|waiting| {
            let mut waiting = waiting.borrow_mut();
            match waiting.iter_mut().find(|(waiter, _)| *waiter == id) {
                Some((_, waker)) => waker.clone_from(context.waker()),
                None => waiting.push((id, context.waker().clone())),
            }
        });
        Poll::Pending
    }
}

impl Drop for PositiveScale {
    fn drop(&mut self) {
        self.leave();
    }
}

/// Waits until the scale here is above 0, as Compose's
/// `snapshotFlow { coroutineContext.durationScale }.first { it > 0f }`: the thread's changes
/// with [`set_motion_duration_scale`]; a task's own (`with_motion_duration_scale`) never does.
pub async fn await_positive_motion_duration_scale() {
    PositiveScale { id: None }.await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::task::Wake;

    /// Counts its wakes.
    #[derive(Default)]
    struct Counted(AtomicU32);

    impl Wake for Counted {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// How many clones of a waker are held elsewhere.
    fn held(waker: &Arc<Counted>) -> usize {
        Arc::strong_count(waker) - 1
    }

    #[test]
    fn a_wait_holds_one_waker_however_often_it_is_polled_and_none_once_dropped() {
        set_motion_duration_scale(0.0);
        let arc = Arc::new(Counted::default());
        let waker = Waker::from(arc.clone());
        let mut context = Context::from_waker(&waker);
        let mut wait = Box::pin(await_positive_motion_duration_scale());
        for _ in 0..5 {
            assert!(wait.as_mut().poll(&mut context).is_pending());
        }
        assert_eq!(held(&arc), 2, "the `Waker` here and the one kept");
        // A task cancelled drops its future.
        drop(wait);
        assert_eq!(held(&arc), 1);
        set_motion_duration_scale(1.0);
    }

    #[test]
    fn a_wait_under_a_task_s_scale_of_0_holds_one_waker_too() {
        let arc = Arc::new(Counted::default());
        let waker = Waker::from(arc.clone());
        let mut context = Context::from_waker(&waker);
        let mut wait =
            Box::pin(with_motion_duration_scale(0.0, await_positive_motion_duration_scale()));
        for _ in 0..5 {
            assert!(wait.as_mut().poll(&mut context).is_pending());
        }
        assert_eq!(held(&arc), 2);
        // Woken by the thread's scale, it waits again, still once.
        set_motion_duration_scale(1.0);
        assert_eq!(held(&arc), 1, "its waker taken");
        assert_eq!(arc.0.load(Ordering::Relaxed), 1, "woken once");
        assert!(wait.as_mut().poll(&mut context).is_pending());
        assert_eq!(held(&arc), 2);
    }

    #[test]
    #[should_panic(expected = "negative scale factor")]
    fn a_negative_scale_of_a_task_panics_at_the_call_not_at_its_first_poll() {
        let _never_polled = with_motion_duration_scale(-1.0, std::future::ready(()));
    }
}
