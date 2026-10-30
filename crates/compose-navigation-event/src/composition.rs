//! In a composition, as AndroidX's `navigationevent-compose`.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use compose_runtime::{
    Callback, ComposeArg, CompositionLocal, DisposableEffect, LaunchedEffect, OwnedState,
    SideEffect, composable, mutable_state_of, remember, remember_keyed,
};

use crate::dispatcher::NavigationEventDispatcher;
use crate::event::{NavigationEvent, NavigationEventInfo, NavigationEventTransitionState};
use crate::handler::{NavigationEventCallbacks, NavigationEventHandler as Handler};

/// The dispatcher of what composes, as AndroidX's `LocalNavigationEventDispatcherOwner`: the owner
/// is the dispatcher, a handle. A host provides its own.
pub static LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER: CompositionLocal<
    Option<NavigationEventDispatcher>,
> = CompositionLocal::new("LocalNavigationEventDispatcherOwner", || None);

/// How many dialogs or focusable popups the content is in: their handlers go first, a dialog here
/// not being a window with a dispatcher of its own (D845).
#[doc(hidden)]
pub static LOCAL_BACK_LAYER: CompositionLocal<u32> = CompositionLocal::new("LocalBackLayer", || 0);

/// A dispatcher of this place in the composition, as `rememberNavigationEventDispatcherOwner`: a
/// child of `parent` (the local one by default; `None`: a root), disposed when it leaves.
#[composable]
pub fn remember_navigation_event_dispatcher_owner(
    #[default(true)] enabled: bool,
    #[default(Some(crate::local_navigation_event_dispatcher_owner()))] parent: Option<
        NavigationEventDispatcher,
    >,
) -> NavigationEventDispatcher {
    let dispatcher = {
        let held = parent.clone();
        remember_keyed(parent, move || match &held {
            Some(parent) => NavigationEventDispatcher::with_parent(parent),
            None => NavigationEventDispatcher::new(),
        })
    };
    let enabling = dispatcher.clone();
    LaunchedEffect(enabled, async move { enabling.set_enabled(enabled) });
    let disposed = dispatcher.clone();
    DisposableEffect(dispatcher.clone(), move |scope| {
        scope.on_dispose(move || {
            // A parent's dispose took it already.
            if !disposed.is_disposed() {
                disposed.dispose();
            }
        })
    });
    dispatcher
}

struct StateInner<T: 'static> {
    transition_state: OwnedState<NavigationEventTransitionState>,
    current_info: OwnedState<T>,
    back_info: OwnedState<Vec<T>>,
    forward_info: OwnedState<Vec<T>>,
    source_handler: RefCell<Option<Handler>>,
}

/// A handler's place and gesture for a composition to read, as AndroidX's `NavigationEventState`:
/// a handle. `T` is any value, with `==`, that a place's [`NavigationEventInfo`] is made of.
pub struct NavigationEventState<T: 'static> {
    inner: Rc<StateInner<T>>,
}

impl<T: 'static> Clone for NavigationEventState<T> {
    fn clone(&self) -> Self {
        NavigationEventState { inner: self.inner.clone() }
    }
}

impl<T: 'static> ComposeArg for NavigationEventState<T> {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<T: 'static> fmt::Debug for NavigationEventState<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "NavigationEventState@{:x}",
            Rc::as_ptr(&self.inner) as *const () as usize
        )
    }
}

impl<T: Clone + PartialEq + fmt::Debug + 'static> NavigationEventState<T> {
    fn new(current_info: T, back_info: Vec<T>, forward_info: Vec<T>) -> NavigationEventState<T> {
        NavigationEventState {
            inner: Rc::new(StateInner {
                transition_state: mutable_state_of(NavigationEventTransitionState::Idle),
                current_info: mutable_state_of(current_info),
                back_info: mutable_state_of(back_info),
                forward_info: mutable_state_of(forward_info),
                source_handler: RefCell::new(None),
            }),
        }
    }

    /// The gesture under way, as `transitionState` (a state read).
    pub fn transition_state(&self) -> NavigationEventTransitionState {
        self.inner.transition_state.get()
    }

    pub fn current_info(&self) -> T {
        self.inner.current_info.get()
    }

    pub fn back_info(&self) -> Vec<T> {
        self.inner.back_info.get()
    }

    pub fn forward_info(&self) -> Vec<T> {
        self.inner.forward_info.get()
    }

    /// Its handler's info, as what's read of the state.
    fn tell(&self, handler: &Handler) {
        let erase = |infos: Vec<T>| infos.into_iter().map(NavigationEventInfo::new).collect();
        handler.set_info(
            NavigationEventInfo::new(self.current_info()),
            erase(self.back_info()),
            erase(self.forward_info()),
        );
    }

    fn update(&self, current_info: T, back_info: Vec<T>, forward_info: Vec<T>) {
        self.inner.current_info.set(current_info);
        self.inner.back_info.set(back_info);
        self.inner.forward_info.set(forward_info);
        let handler = self.inner.source_handler.borrow().clone();
        if let Some(handler) = handler {
            self.tell(&handler);
        }
    }
}

/// The state of a place, as `rememberNavigationEventState`: updated with the info given once the
/// pass is applied, and so is the handler it's registered with.
#[composable]
pub fn remember_navigation_event_state<T: Clone + PartialEq + fmt::Debug + 'static>(
    current_info: T,
    #[default(Vec::new())] back_info: Vec<T>,
    #[default(Vec::new())] forward_info: Vec<T>,
) -> NavigationEventState<T> {
    let state = {
        let (current, back, forward) =
            (current_info.clone(), back_info.clone(), forward_info.clone());
        remember(move || NavigationEventState::new(current, back, forward))
    };
    let updated = state.clone();
    SideEffect(move || updated.update(current_info, back_info, forward_info));
    state
}

struct ComposeCallbacks {
    on_transition_state_changed: Box<dyn Fn(NavigationEventTransitionState)>,
    current: Rc<RefCell<CurrentCallbacks>>,
}

struct CurrentCallbacks {
    on_forward_cancelled: Callback,
    on_forward_completed: Callback,
    on_back_cancelled: Callback,
    on_back_completed: Callback,
}

impl NavigationEventCallbacks for ComposeCallbacks {
    fn on_back_started(&self, handler: &Handler, _: &NavigationEvent) {
        (self.on_transition_state_changed)(handler.transition_state());
    }

    fn on_back_progressed(&self, handler: &Handler, _: &NavigationEvent) {
        (self.on_transition_state_changed)(handler.transition_state());
    }

    fn on_back_completed(&self, handler: &Handler) {
        (self.on_transition_state_changed)(handler.transition_state());
        let callback = self.current.borrow().on_back_completed.clone();
        callback();
    }

    fn on_back_cancelled(&self, handler: &Handler) {
        (self.on_transition_state_changed)(handler.transition_state());
        let callback = self.current.borrow().on_back_cancelled.clone();
        callback();
    }

    fn on_forward_started(&self, handler: &Handler, _: &NavigationEvent) {
        (self.on_transition_state_changed)(handler.transition_state());
    }

    fn on_forward_progressed(&self, handler: &Handler, _: &NavigationEvent) {
        (self.on_transition_state_changed)(handler.transition_state());
    }

    fn on_forward_completed(&self, handler: &Handler) {
        (self.on_transition_state_changed)(handler.transition_state());
        let callback = self.current.borrow().on_forward_completed.clone();
        callback();
    }

    fn on_forward_cancelled(&self, handler: &Handler) {
        (self.on_transition_state_changed)(handler.transition_state());
        let callback = self.current.borrow().on_forward_cancelled.clone();
        callback();
    }
}

/// A place taking the gestures, with the local dispatcher, as AndroidX's composable
/// `NavigationEventHandler`: `state` tells where it is and hears the gesture.
#[composable]
#[allow(clippy::too_many_arguments)]
pub fn NavigationEventHandler<T: Clone + PartialEq + fmt::Debug + 'static>(
    state: NavigationEventState<T>,
    #[default(true)] is_forward_enabled: bool,
    #[default(|| {})] on_forward_cancelled: impl Fn() + 'static,
    #[default(|| {})] on_forward_completed: impl Fn() + 'static,
    #[default(true)] is_back_enabled: bool,
    #[default(|| {})] on_back_cancelled: impl Fn() + 'static,
    #[default(|| {})] on_back_completed: impl Fn() + 'static,
) {
    let dispatcher = LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.current().expect(
        "No NavigationEventDispatcher was provided via LocalNavigationEventDispatcherOwner",
    );
    let (source_handler, current) = {
        let read = state.clone();
        remember_keyed(state.clone(), move || {
            let none = || compose_runtime::callback(|| {});
            let current = Rc::new(RefCell::new(CurrentCallbacks {
                on_forward_cancelled: none(),
                on_forward_completed: none(),
                on_back_cancelled: none(),
                on_back_completed: none(),
            }));
            let told = read.clone();
            let handler = Handler::new(
                NavigationEventInfo::new(read.current_info()),
                false,
                false,
                ComposeCallbacks {
                    on_transition_state_changed: Box::new(move |transition_state| {
                        told.inner.transition_state.set(transition_state);
                    }),
                    current: current.clone(),
                },
            );
            (handler, current)
        })
    };
    let (told, handler) = (state.clone(), source_handler.clone());
    SideEffect(move || {
        handler.set_forward_enabled(is_forward_enabled);
        *current.borrow_mut() = CurrentCallbacks {
            on_forward_cancelled,
            on_forward_completed,
            on_back_cancelled,
            on_back_completed,
        };
        handler.set_back_enabled(is_back_enabled);
        told.tell(&handler);
    });
    let held = state.clone();
    let added = source_handler;
    let layer = LOCAL_BACK_LAYER.current();
    DisposableEffect(state, move |scope| {
        let previous = held.inner.source_handler.borrow().clone();
        assert!(
            previous.is_none(),
            "NavigationEventState '{held:?}' is already registered with a NavigationEventHandler \
             '{added:?}'."
        );
        *held.inner.source_handler.borrow_mut() = Some(added.clone());
        dispatcher.add_handler_in_layer(&added, layer);
        scope.on_dispose(move || {
            added.remove();
            *held.inner.source_handler.borrow_mut() = None;
        })
    });
}

/// [`NavigationEventHandler`] of back gestures alone, as `NavigationBackHandler`.
#[composable]
pub fn NavigationBackHandler<T: Clone + PartialEq + fmt::Debug + 'static>(
    state: NavigationEventState<T>,
    #[default(true)] is_back_enabled: bool,
    #[default(|| {})] on_back_cancelled: impl Fn() + 'static,
    on_back_completed: impl Fn() + 'static,
) {
    NavigationEventHandler!(
        state,
        is_forward_enabled = false,
        is_back_enabled = is_back_enabled,
        on_back_cancelled = move || on_back_cancelled(),
        on_back_completed = move || on_back_completed()
    );
}

/// [`NavigationEventHandler`] of forward gestures alone, as `NavigationForwardHandler`.
#[composable]
pub fn NavigationForwardHandler<T: Clone + PartialEq + fmt::Debug + 'static>(
    state: NavigationEventState<T>,
    #[default(true)] is_forward_enabled: bool,
    #[default(|| {})] on_forward_cancelled: impl Fn() + 'static,
    on_forward_completed: impl Fn() + 'static,
) {
    NavigationEventHandler!(
        state,
        is_back_enabled = false,
        is_forward_enabled = is_forward_enabled,
        on_forward_cancelled = move || on_forward_cancelled(),
        on_forward_completed = move || on_forward_completed()
    );
}
