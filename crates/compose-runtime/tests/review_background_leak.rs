//! Review of D231: a worker's panic outside `catch_unwind` (own binary: it breaks the pool).

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
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

#[test]
fn a_result_whose_drop_panics_on_a_worker_keeps_the_pool_working() {
    // A cancelled future's result is dropped on the worker, outside `catch_unwind`.
    struct PanicOnDrop;
    impl Drop for PanicOnDrop {
        fn drop(&mut self) {
            panic!("drop panics");
        }
    }
    let (release, released) = mpsc::channel::<()>();
    let released = Arc::new(Mutex::new(released));
    let (started_sender, started) = mpsc::channel::<()>();
    let futures: Vec<_> = (0..64)
        .map(|_| {
            let (released, started) = (released.clone(), started_sender.clone());
            spawn_background(move || {
                started.send(()).unwrap();
                let _ = released.lock().unwrap().recv();
                PanicOnDrop
            })
        })
        .collect();
    for _ in 0..64 {
        started.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    drop(futures);
    for _ in 0..64 {
        release.send(()).unwrap();
    }
    std::thread::sleep(Duration::from_millis(500));
    // Had each worker died in the drop's panic without giving its slot back, the pool would
    // believe 64 threads run and never start another.
    let mut after = spawn_background(|| 1);
    let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current(), AtomicUsize::new(0))));
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut done = false;
    while Instant::now() < deadline {
        if poll(&mut after, &waker).is_ready() {
            done = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(done, "the pool is stuck: no thread will ever run new work");
}
