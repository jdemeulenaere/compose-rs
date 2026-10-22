//! Side effects (docs/guide/kotlin-to-rust.md, "Side effects").

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: launched
#[composable]
pub fn Countdown(from: u32) {
    let left = remember(|| mutable_state_of(from));
    // Restarted (the old task cancelled) whenever `from` changes; cancelled when this leaves.
    LaunchedEffect(from, async move {
        left.set(from);
        while left.get() > 0 {
            delay(Duration::from_secs(1)).await;
            left.update(|seconds| seconds - 1);
        }
    });
    Text!("{} s", left.get());
}
// ANCHOR_END: launched

/// A message bus, for the `DisposableEffect` sample.
#[derive(Default)]
pub struct Bus {
    listeners: RefCell<Vec<(u64, SharedString)>>,
    next: std::cell::Cell<u64>,
}

impl Bus {
    pub fn subscribe(&self, topic: &SharedString) -> u64 {
        let id = self.next.get();
        self.next.set(id + 1);
        self.listeners.borrow_mut().push((id, topic.clone()));
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        self.listeners.borrow_mut().retain(|(listener, _)| *listener != id);
    }

    pub fn topics(&self) -> Vec<SharedString> {
        self.listeners.borrow().iter().map(|(_, topic)| topic.clone()).collect()
    }
}

// ANCHOR: disposable
#[composable]
pub fn Subscription(topic: SharedString, bus: Rc<Bus>) {
    // Subscribes after the first composition, and again (unsubscribing first) for a new topic.
    DisposableEffect(topic.clone(), move |scope| {
        let id = bus.subscribe(&topic);
        scope.on_dispose(move || bus.unsubscribe(id))
    });
}
// ANCHOR_END: disposable

/// What the `SideEffect` sample reports to.
#[derive(Default)]
pub struct Analytics {
    pub screen: RefCell<String>,
}

// ANCHOR: side_effect
#[composable]
pub fn Tracked(screen: SharedString, analytics: Rc<Analytics>) {
    Text!(screen.clone());
    // After every composition of this scope that completes.
    SideEffect(move || *analytics.screen.borrow_mut() = screen.to_string());
}
// ANCHOR_END: side_effect

// ANCHOR: background
#[composable]
pub fn FileSize(path: String) {
    let size = remember(|| mutable_state_of(None::<u64>));
    LaunchedEffect(path.clone(), async move {
        // Blocking work runs on a thread pool (`Dispatchers.IO`); the closure is `Send`, so
        // it takes plain data, not state handles. The task resumes on the UI thread.
        let bytes = spawn_background(move || std::fs::metadata(path).ok().map(|file| file.len()));
        size.set(bytes.await);
    });
    match size.get() {
        Some(bytes) => Text!("{} bytes", bytes),
        None => IndeterminateCircularProgressIndicator!(),
    }
}
// ANCHOR_END: background

// ANCHOR: scope
#[composable]
pub fn Saver() {
    let scope = remember_coroutine_scope();
    let snackbars = remember(SnackbarHostState::new);
    Column!({
        Button!(
            on_click = move || {
                // Launched from a handler; cancelled when `Saver` leaves the composition.
                scope.launch(async move {
                    snackbars.show_snackbar("Saved").await;
                });
            },
            {
                Text!("Save");
            }
        );
        SnackbarHost!(snackbars);
    });
}
// ANCHOR_END: scope
