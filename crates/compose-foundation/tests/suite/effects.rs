//! `LaunchedEffect`, `remember_coroutine_scope`, `delay` and the frame clock, through the test
//! rule's main clock.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// Logs when the task holding it is dropped: a cancelled task drops its future.
struct DropLog(&'static str);

impl Drop for DropLog {
    fn drop(&mut self) {
        log(format!("dropped {}", self.0));
    }
}

#[test]
fn a_launched_effect_runs_after_the_pass_and_its_writes_recompose() {
    #[composable]
    fn Screen() {
        let text = remember(|| mutable_state_of(String::from("loading")));
        LaunchedEffect((), async move {
            text.set(String::from("loaded"));
        });
        BasicText!(text.get());
    }
    let mut ui = compose_test(Screen);
    ui.on_node_with_text("loaded").assert_exists();
}

#[test]
fn delay_waits_for_the_test_clock() {
    #[composable]
    fn Screen() {
        let shown = remember(|| mutable_state_of(false));
        LaunchedEffect((), async move {
            delay(Duration::from_secs(1)).await;
            shown.set(true);
        });
        if shown.get() {
            BasicText!("shown");
        }
    }
    let mut ui = compose_test(Screen);
    // The clock advancing on its own runs frames while they're needed, not past delays, as
    // Compose's (D900): the delay waits for the clock.
    ui.on_node_with_text("shown").assert_does_not_exist();
    ui.advance_time_by(Duration::from_millis(1000));
    ui.on_node_with_text("shown").assert_exists();

    let mut ui = ComposeTestRule::with_manual_clock(Screen);
    assert_eq!(ui.current_time_millis(), 0);
    ui.on_node_with_text("shown").assert_does_not_exist();
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(999));
    ui.on_node_with_text("shown").assert_does_not_exist();
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(1));
    // The state is written at 1000 ms; the frame that shows it comes 16 ms later.
    ui.on_node_with_text("shown").assert_does_not_exist();
    ui.advance_time_by_frame();
    ui.on_node_with_text("shown").assert_exists();
    assert_eq!(ui.current_time_millis(), 1016);
}

#[composable]
fn Keyed(key: u32) {
    LaunchedEffect(key, async move {
        let _guard = DropLog(if key == 1 { "one" } else { "two" });
        log(format!("started {key}"));
        // Never ends on its own.
        std::future::pending::<()>().await;
    });
}

#[test]
fn a_key_change_cancels_the_task_and_starts_the_new_one() {
    take_log();
    let key = mutable_state_of(1);
    let handle = key.handle();
    let mut ui = compose_test(move || Keyed(handle.get()));
    assert_eq!(take_log(), ["started 1"]);
    key.set(2);
    ui.wait_for_idle();
    assert_eq!(take_log(), ["dropped one", "started 2"]);
    // The same key: the task goes on.
    key.set(2);
    ui.wait_for_idle();
    assert_eq!(take_log(), Vec::<String>::new());
}

#[test]
fn leaving_the_composition_cancels_the_task() {
    take_log();
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut ui = compose_test(move || {
        if handle.get() {
            Keyed(1);
        }
    });
    assert_eq!(take_log(), ["started 1"]);
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(take_log(), ["dropped one"]);
}

#[test]
fn a_scope_launches_from_a_handler_and_is_cancelled_with_its_caller() {
    take_log();
    #[composable]
    fn Screen(shown: MutableState<bool>) {
        if shown.get() {
            let scope = remember_coroutine_scope();
            let count = remember(|| mutable_state_of(0));
            Box!(
                modifier = Modifier
                    .clickable(move || {
                        scope.launch(async move {
                            let _guard = DropLog("launched");
                            delay(Duration::from_millis(100)).await;
                            count.update(|count| count + 1);
                            std::future::pending::<()>().await;
                        });
                    })
                    .test_tag("button"),
                {
                    BasicText!("{}", count.get());
                }
            );
        }
    }
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut ui = compose_test(move || Screen(handle));
    ui.on_node_with_tag("button").perform_click();
    // The task's delay waits for the clock (D900).
    ui.advance_time_by(Duration::from_millis(100));
    ui.on_node_with_text("1").assert_exists();
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(take_log(), ["dropped launched"]);
}

#[test]
fn with_frame_nanos_resumes_once_per_frame_with_the_frame_time() {
    #[composable]
    fn Animated() {
        let frames = remember(|| mutable_state_of(Vec::<u64>::new()));
        LaunchedEffect((), async move {
            for _ in 0..3 {
                let time = with_frame_millis(|millis| millis).await;
                frames.edit(|frames| frames.push(time));
            }
        });
        let frames = frames.get();
        let steps: Vec<u64> = frames.windows(2).map(|pair| pair[1] - pair[0]).collect();
        BasicText!("{} frames, steps {:?}", frames.len(), steps);
    }
    let mut ui = compose_test(Animated);
    ui.on_node_with_text("3 frames, steps [16, 16]").assert_exists();
}

/// A one-shot channel completed from another thread: what background work posts back.
#[derive(Default)]
struct Shared {
    value: Option<u32>,
    waker: Option<Waker>,
}

struct Receive(Arc<Mutex<Shared>>);

impl Future for Receive {
    type Output = u32;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<u32> {
        let mut shared = self.0.lock().unwrap();
        match shared.value.take() {
            Some(value) => Poll::Ready(value),
            None => {
                shared.waker = Some(context.waker().clone());
                Poll::Pending
            }
        }
    }
}

#[test]
fn a_task_woken_from_another_thread_resumes() {
    #[composable]
    fn Screen(channel: Arc<Mutex<Shared>>) {
        let result = remember(|| mutable_state_of(0));
        LaunchedEffect((), async move {
            let value = Receive(channel).await;
            result.set(value);
        });
        BasicText!("result {}", result.get());
    }
    let channel = Arc::new(Mutex::new(Shared::default()));
    let given = channel.clone();
    let mut ui = compose_test(move || Screen(given.clone()));
    ui.on_node_with_text("result 0").assert_exists();
    std::thread::spawn(move || {
        let waker = {
            let mut shared = channel.lock().unwrap();
            shared.value = Some(42);
            shared.waker.take()
        };
        waker.expect("the task waits").wake();
    })
    .join()
    .unwrap();
    ui.wait_for_idle();
    ui.on_node_with_text("result 42").assert_exists();
}

#[test]
fn produce_state_gives_what_its_coroutine_sets() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        let loaded = produce_state(String::from("Loading"), (), |state| async move {
            delay(std::time::Duration::from_millis(100)).await;
            state.set(String::from("Loaded"));
        });
        BasicText!("{}", loaded.get());
    });
    ui.on_node_with_text("Loading").assert_exists();
    ui.advance_time_by(std::time::Duration::from_millis(150));
    ui.on_node_with_text("Loaded").assert_exists();
}

#[test]
fn an_effect_calls_the_latest_callback_through_remember_updated_state() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let calls: Rc<RefCell<Vec<i32>>> = Rc::default();
    let seen = calls.clone();
    let version = mutable_state_of(1);
    let handle = version.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let current = handle.get();
        let seen = seen.clone();
        let on_timeout = remember_updated_state(
            Rc::new(move || seen.borrow_mut().push(current)) as Rc<dyn Fn()>
        );
        LaunchedEffect((), async move {
            delay(std::time::Duration::from_millis(100)).await;
            (on_timeout.get())();
        });
    });
    version.set(2);
    ui.advance_time_by(std::time::Duration::from_millis(150));
    assert_eq!(*calls.borrow(), vec![2], "the newest callback, from the first launch");
}

/// Frames until `text` shows, the background threads given real time to finish.
fn wait_for_text(ui: &mut ComposeTestRule, text: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while ui.count_nodes(&has_text(text)) == 0 {
        assert!(std::time::Instant::now() < deadline, "{text:?} never showed");
        std::thread::sleep(Duration::from_millis(1));
        ui.wait_for_idle();
    }
}

#[test]
fn a_launched_effect_awaiting_background_work_writes_its_result() {
    #[composable]
    fn Screen() {
        let text = remember(|| mutable_state_of(String::from("loading")));
        LaunchedEffect((), async move {
            let main = std::thread::current().id();
            let (loaded, thread) = spawn_background(|| {
                std::thread::sleep(Duration::from_millis(5));
                (String::from("loaded"), std::thread::current().id())
            })
            .await;
            assert_ne!(thread, main, "the work ran on another thread");
            assert_eq!(std::thread::current().id(), main, "the task resumed on the UI thread");
            text.set(loaded);
        });
        BasicText!(text.get());
    }
    let mut ui = compose_test(Screen);
    ui.on_node_with_text("loading").assert_exists();
    wait_for_text(&mut ui, "loaded");
}

#[test]
fn an_effect_leaving_the_composition_discards_its_background_result() {
    struct Probe(std::sync::mpsc::Sender<std::thread::ThreadId>);
    impl Drop for Probe {
        fn drop(&mut self) {
            let _ = self.0.send(std::thread::current().id());
        }
    }
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let released = Arc::new(Mutex::new(released));
    let (dropped_sender, dropped) = std::sync::mpsc::channel::<std::thread::ThreadId>();
    let (started_sender, started) = std::sync::mpsc::channel::<()>();
    let mut ui = compose_test(move || {
        let text = remember(|| mutable_state_of(String::from("waiting")));
        if handle.get() {
            let (released, dropped_sender) = (released.clone(), dropped_sender.clone());
            let started_sender = started_sender.clone();
            LaunchedEffect((), async move {
                let probe = spawn_background(move || {
                    started_sender.send(()).unwrap();
                    let _ = released.lock().unwrap().recv();
                    Probe(dropped_sender)
                })
                .await;
                drop(probe);
                text.set(String::from("written"));
            });
        }
        BasicText!(text.get());
    });
    // The effect leaves while the work blocks: its task is cancelled without waiting for it.
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    shown.set(false);
    ui.wait_for_idle();
    release.send(()).unwrap();
    let thread = dropped.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_ne!(thread, std::thread::current().id(), "dropped on the background thread");
    ui.wait_for_idle();
    ui.on_node_with_text("waiting").assert_exists();
}
