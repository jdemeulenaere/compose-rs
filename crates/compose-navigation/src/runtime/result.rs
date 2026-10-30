//! `result/`: results passed between entries.

use std::any::{Any, type_name};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::rc::Rc;
use std::task::{Poll, Waker};

use compose_runtime::snapshots::{OwnedStateMap, mutable_state_map_of};
use compose_runtime::{
    ComposeArg, CompositionLocal, CompositionLocalProvider, LaunchedEffect, State, composable,
    mutable_state_of, remember, remember_keyed, remember_updated_state,
};

use super::decorator::NavEntryDecorator;
use super::entry::NavEntry;

/// `Channel.BUFFERED`.
const BUFFERED: usize = 64;

/// A key's results, as its `Channel<Any?>`.
#[derive(Default)]
struct ResultChannel {
    results: RefCell<VecDeque<Rc<dyn Any>>>,
    closed: Cell<bool>,
    wakers: RefCell<Vec<Waker>>,
}

impl ResultChannel {
    fn wake(&self) {
        let wakers = std::mem::take(&mut *self.wakers.borrow_mut());
        for waker in wakers {
            waker.wake();
        }
    }

    fn try_send(&self, result: Rc<dyn Any>) {
        if self.closed.get() {
            return;
        }
        let mut results = self.results.borrow_mut();
        if results.len() < BUFFERED {
            results.push_back(result);
            drop(results);
            self.wake();
        }
    }

    fn close(&self) {
        self.closed.set(true);
        self.wake();
    }

    /// The next result: `None` once closed and read to its end, as a channel's.
    async fn next(&self) -> Option<Rc<dyn Any>> {
        let mut waiting = Waiting { channel: self, waker: None };
        std::future::poll_fn(|context| {
            if let Some(result) = self.results.borrow_mut().pop_front() {
                return Poll::Ready(Some(result));
            }
            if self.closed.get() {
                return Poll::Ready(None);
            }
            waiting.register(context.waker());
            Poll::Pending
        })
        .await
    }
}

/// A wait for a result: its waker stays once in the channel's, and goes when it's dropped, a task
/// cancelled leaving nothing to wake.
struct Waiting<'a> {
    channel: &'a ResultChannel,
    waker: Option<Waker>,
}

impl Waiting<'_> {
    fn register(&mut self, waker: &Waker) {
        let mut wakers = self.channel.wakers.borrow_mut();
        if let Some(old) = self.waker.take() {
            wakers.retain(|other| !other.will_wake(&old));
        }
        if !wakers.iter().any(|other| other.will_wake(waker)) {
            wakers.push(waker.clone());
        }
        self.waker = Some(waker.clone());
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        if let Some(waker) = &self.waker {
            self.channel.wakers.borrow_mut().retain(|other| !other.will_wake(waker));
        }
    }
}

/// A channel in the state map, by identity.
#[derive(Clone)]
struct ChannelRef(Rc<ResultChannel>);

impl PartialEq for ChannelRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ComposeArg for ChannelRef {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Passes results between screens, as `ResultEventBus`: a handle. A [`ResultEffect`] hears each
/// result of its key, one state of [`conflate_as_state`](Self::conflate_as_state) the latest. A
/// key is a type's name, or one of the app's, so a `&str` sent isn't heard by what waits for a
/// `String`, which are other types' keys. Results aren't saved.
#[derive(Clone)]
pub struct ResultEventBus {
    channel_map: Rc<OwnedStateMap<String, ChannelRef>>,
}

impl Default for ResultEventBus {
    #[track_caller]
    fn default() -> ResultEventBus {
        ResultEventBus::new()
    }
}

impl ComposeArg for ResultEventBus {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.channel_map, &other.channel_map)
    }
}

fn key_of<T>() -> String {
    type_name::<T>().to_string()
}

impl ResultEventBus {
    #[track_caller]
    pub fn new() -> ResultEventBus {
        ResultEventBus { channel_map: Rc::new(mutable_state_map_of()) }
    }

    fn channel(&self, result_key: &str) -> ChannelRef {
        let key = result_key.to_string();
        if let Some(channel) = self.channel_map.get(&key) {
            return channel;
        }
        let channel = ChannelRef(Rc::default());
        self.channel_map.insert_always(key, channel.clone());
        channel
    }

    /// Sends `result`, under its type's key. Past 64 waiting, it's dropped, as `trySend`.
    pub fn send_result<T: 'static>(&self, result: T) {
        self.send_result_to(&key_of::<T>(), result);
    }

    /// Sends `result` under `result_key`.
    pub fn send_result_to<T: 'static>(&self, result_key: &str, result: T) {
        self.channel(result_key).0.try_send(Rc::new(result));
    }

    /// Forgets the results of `T`'s key, and ends what's waiting for them.
    pub fn remove_result<T: 'static>(&self) {
        self.remove_result_of(&key_of::<T>());
    }

    /// Forgets the results of `result_key`, and ends what's waiting for them.
    pub fn remove_result_of(&self, result_key: &str) {
        if let Some(channel) = self.channel_map.remove(&result_key.to_string()) {
            channel.0.close();
        }
    }

    /// A state of the latest result of `T`'s key, `default_value` until there's one, as
    /// `conflateAsState`.
    pub fn conflate_as_state<T: Clone + PartialEq + 'static>(&self, default_value: T) -> State<T> {
        ConflatedResult(self.clone(), key_of::<T>(), default_value)
    }

    /// The same, of `result_key`.
    pub fn conflate_as_state_of<T: Clone + PartialEq + 'static>(
        &self,
        result_key: &str,
        default_value: T,
    ) -> State<T> {
        ConflatedResult(self.clone(), result_key.to_string(), default_value)
    }
}

#[composable(non_restartable)]
#[allow(non_snake_case)]
fn ConflatedResult<T: Clone + PartialEq + 'static>(
    bus: ResultEventBus,
    result_key: String,
    default_value: T,
) -> State<T> {
    let state = remember(|| mutable_state_of(default_value));
    let channel = {
        let (bus, result_key) = (bus.clone(), result_key.clone());
        remember_keyed(result_key.clone(), move || bus.channel(&result_key))
    };
    let held = channel.clone();
    LaunchedEffect(channel, async move {
        while let Some(result) = held.0.next().await {
            state.set(result.downcast_ref::<T>().expect("a result of its key's type").clone());
        }
    });
    state.as_state()
}

/// The [`ResultEventBus`] of a place in the composition, as `LocalResultEventBus`.
pub struct LocalResultEventBus;

static LOCAL_RESULT_EVENT_BUS: CompositionLocal<Option<ResultEventBus>> =
    CompositionLocal::new("LocalResultEventBus", || None);

impl LocalResultEventBus {
    /// The current one. Panics if none's been provided.
    pub fn current() -> ResultEventBus {
        LOCAL_RESULT_EVENT_BUS.current().expect("No ResultEventBus has been provided")
    }

    /// Provides `bus`.
    pub fn provides(bus: ResultEventBus) -> compose_runtime::ProvidedValue<Option<ResultEventBus>> {
        LOCAL_RESULT_EVENT_BUS.provides(Some(bus))
    }
}

/// A bus kept while this place is, as `rememberResultEventBus`.
#[composable]
pub fn remember_result_event_bus() -> ResultEventBus {
    remember(ResultEventBus::new)
}

/// Runs `on_result` with each result of its key, as `ResultEffect`: the latest closure given. Its
/// key is `result_key`, or `T`'s name without one, and its bus the local one by default.
#[composable]
#[allow(non_snake_case)]
pub fn ResultEffect<T: 'static, F: Future<Output = ()> + 'static>(
    #[default(None)] result_key: Option<impl Into<String>>,
    #[default(LocalResultEventBus::current())] result_event_bus: ResultEventBus,
    on_result: impl Fn(T) -> F + 'static,
) {
    let result_key = result_key.unwrap_or_else(key_of::<T>);
    let current = remember_updated_state(on_result);
    let channel = result_event_bus.channel_map.get(&result_key);
    let bus = result_event_bus.clone();
    let key = result_key.clone();
    LaunchedEffect((result_key, channel), async move {
        let channel = bus.channel(&key);
        while let Some(result) = channel.0.next().await {
            let result = result.downcast::<T>().expect("a result of its key's type");
            let result = Rc::try_unwrap(result).ok().expect("a result is heard once");
            (current.get())(result).await;
        }
    });
}

/// Provides `bus` to each entry, as `ResultEventBusNavEntryDecorator`, so one can pass results to
/// those before it.
pub struct ResultEventBusNavEntryDecorator;

impl ResultEventBusNavEntryDecorator {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<T: Clone + 'static>(bus: ResultEventBus) -> NavEntryDecorator<T> {
        NavEntryDecorator::new(move |entry: NavEntry<T>| {
            CompositionLocalProvider(LocalResultEventBus::provides(bus.clone()), move || {
                entry.Content();
            });
        })
    }
}

/// The decorator of `result_event_bus`, kept while this place is, as
/// `rememberResultEventBusNavEntryDecorator`.
#[composable]
pub fn remember_result_event_bus_nav_entry_decorator<T: Clone + 'static>(
    #[default(remember_result_event_bus())] result_event_bus: ResultEventBus,
) -> NavEntryDecorator<T> {
    remember(|| ResultEventBusNavEntryDecorator::new(result_event_bus))
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::task::{Context, Waker};

    use super::*;

    /// A wait given up leaves no waker behind, and one polled again stays once.
    #[test]
    fn a_wait_leaves_one_waker_and_takes_it_when_dropped() {
        let channel = ResultChannel::default();
        let mut context = Context::from_waker(Waker::noop());
        {
            let mut next = std::pin::pin!(channel.next());
            assert!(next.as_mut().poll(&mut context).is_pending());
            assert!(next.as_mut().poll(&mut context).is_pending());
            assert_eq!(channel.wakers.borrow().len(), 1);
        }
        assert!(channel.wakers.borrow().is_empty());
    }
}
