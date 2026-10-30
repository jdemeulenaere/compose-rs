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
    /// What waits for the thread's scale to be above 0.
    static WAITING: RefCell<Vec<Waker>> = const { RefCell::new(Vec::new()) };
}

/// The scale animations run at here, as Compose's `coroutineContext.durationScale`: the
/// innermost [`with_motion_duration_scale`] around the code polled, else the thread's (1 unless
/// [`set_motion_duration_scale`] set another).
pub fn motion_duration_scale() -> f32 {
    SCOPED_SCALE.with(Cell::get).unwrap_or_else(|| DEFAULT_SCALE.with(Cell::get))
}

/// Sets the thread's scale, as a host's `MotionDurationScale` (Android's animator scale) or a
/// test's composition `effectContext`: animations running read it at their next frame, and
/// those waiting for a scale above 0 (an infinite transition at 0) go on. Panics if it is
/// negative, as Compose's.
pub fn set_motion_duration_scale(scale: f32) {
    assert!(scale >= 0.0 || scale.is_nan(), "negative scale factor");
    DEFAULT_SCALE.with(|default| default.set(scale));
    if scale > 0.0 {
        let waiting = WAITING.with(|waiting| std::mem::take(&mut *waiting.borrow_mut()));
        for waker in waiting {
            waker.wake();
        }
    }
}

/// `future` with a scale of its own while it is polled, as Compose's
/// `withContext(MotionDurationScale)`. Panics if `scale` is negative.
pub async fn with_motion_duration_scale<F: Future>(scale: f32, future: F) -> F::Output {
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
    .await
}

/// Waits until the scale here is above 0, as Compose's
/// `snapshotFlow { coroutineContext.durationScale }.first { it > 0f }`: the thread's changes
/// with [`set_motion_duration_scale`]; a task's own (`with_motion_duration_scale`) never does.
pub async fn await_positive_motion_duration_scale() {
    std::future::poll_fn(|context: &mut Context<'_>| {
        if motion_duration_scale() > 0.0 {
            Poll::Ready(())
        } else {
            WAITING.with(|waiting| waiting.borrow_mut().push(context.waker().clone()));
            Poll::Pending
        }
    })
    .await
}
