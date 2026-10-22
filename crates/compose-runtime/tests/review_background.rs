//! Review of D231: `spawn_background` under load.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};

use compose_runtime::{Background, spawn_background};

/// Wakes the test thread.
struct ThreadWaker(std::thread::Thread, AtomicUsize);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.unpark();
    }
}

fn poll<T>(future: &mut Background<T>, waker: &Waker) -> Poll<T> {
    Pin::new(future).poll(&mut Context::from_waker(waker))
}

/// Polls every future until all are ready (parking between rounds), within `limit`.
fn block_on_all<T>(mut futures: Vec<Background<T>>, limit: Duration) -> Vec<T> {
    let waker = Arc::new(ThreadWaker(std::thread::current(), AtomicUsize::new(0)));
    let as_waker = Waker::from(waker.clone());
    let mut results: Vec<Option<T>> = futures.iter().map(|_| None).collect();
    let deadline = Instant::now() + limit;
    loop {
        let mut pending = 0;
        for (index, future) in futures.iter_mut().enumerate() {
            if results[index].is_some() {
                continue;
            }
            match poll(future, &as_waker) {
                Poll::Ready(value) => results[index] = Some(value),
                Poll::Pending => pending += 1,
            }
        }
        if pending == 0 {
            break;
        }
        assert!(Instant::now() < deadline, "{pending} never completed");
        std::thread::park_timeout(Duration::from_millis(10));
    }
    futures.clear();
    results.into_iter().map(Option::unwrap).collect()
}

#[test]
fn thousands_of_concurrent_spawns_complete_within_the_thread_limit() {
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let futures: Vec<_> = (0..5000u32)
        .map(|index| {
            let (running, peak) = (running.clone(), peak.clone());
            spawn_background(move || {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                if index % 50 == 0 {
                    std::thread::sleep(Duration::from_millis(2));
                }
                running.fetch_sub(1, Ordering::SeqCst);
                index * 2
            })
        })
        .collect();
    let results = block_on_all(futures, Duration::from_secs(60));
    assert_eq!(results, (0..5000u32).map(|index| index * 2).collect::<Vec<_>>());
    assert!(peak.load(Ordering::SeqCst) <= 64);
}

#[test]
fn spawns_from_many_threads_at_once_all_complete() {
    let handles: Vec<_> = (0..16)
        .map(|thread| {
            std::thread::spawn(move || {
                let futures: Vec<_> = (0..300u32)
                    .map(|index| spawn_background(move || thread * 1000 + index))
                    .collect();
                let results = block_on_all(futures, Duration::from_secs(60));
                assert_eq!(
                    results,
                    (0..300u32).map(|index| thread * 1000 + index).collect::<Vec<_>>()
                );
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn dropping_many_futures_mid_flight_drops_each_result_once() {
    struct Counted(Arc<AtomicUsize>);
    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let dropped = Arc::new(AtomicUsize::new(0));
    let created = Arc::new(AtomicUsize::new(0));
    for round in 0..2000 {
        let (dropped, created) = (dropped.clone(), created.clone());
        let future = spawn_background(move || {
            created.fetch_add(1, Ordering::SeqCst);
            Counted(dropped)
        });
        if round % 3 == 0 {
            std::thread::yield_now();
        }
        drop(future);
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        // Every result created was dropped (none leaked in a shared slot).
        let (made, gone) = (created.load(Ordering::SeqCst), dropped.load(Ordering::SeqCst));
        if made == gone {
            // Settle: no work still running.
            std::thread::sleep(Duration::from_millis(50));
            if created.load(Ordering::SeqCst) == dropped.load(Ordering::SeqCst) {
                break;
            }
        }
        assert!(Instant::now() < deadline, "made {made}, dropped {gone}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn polling_after_completion_panics() {
    let mut future = spawn_background(|| 7);
    let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current(), AtomicUsize::new(0))));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Poll::Ready(value) = poll(&mut future, &waker) {
            assert_eq!(value, 7);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    // Polled again: a clear panic, as std's futures (not Pending forever: a task would hang).
    let again =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| poll(&mut future, &waker)));
    assert!(again.is_err());
}
