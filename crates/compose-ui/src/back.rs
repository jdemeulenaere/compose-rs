//! Back navigation, as AndroidX's `OnBackPressedDispatcher` and `BackHandler`: the host owns a
//! dispatcher ([`LOCAL_ON_BACK_PRESSED_DISPATCHER`]) where `BackHandler`s register; a back
//! press (Android's back gesture or button, Escape on desktop when nothing focused takes it)
//! calls the innermost enabled one. The platform asks [`has_enabled_callbacks`] after each
//! frame, so it intercepts back only while a handler wants it (otherwise Android's activity
//! finishes, as usual).
//!
//! Handlers in a dialog or a focusable popup come first, as Compose's dialogs are windows of
//! their own that get back before the window beneath; among the others, the one registered last
//! (composed last) wins, as AndroidX's dispatcher.
//!
//! [`has_enabled_callbacks`]: OnBackPressedDispatcher::has_enabled_callbacks

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::{
    Callback, CompositionLocal, RememberObserver, SideEffect, composable, remember_observer,
};

/// A registered handler.
struct BackCallback {
    enabled: Cell<bool>,
    /// How many dialogs or focusable popups it's in: the deepest goes first.
    layer: u32,
    /// The order it was registered in: the last goes first, within a layer.
    order: u64,
    on_back: RefCell<Callback>,
}

#[derive(Default)]
struct DispatcherInner {
    callbacks: RefCell<Vec<Rc<BackCallback>>>,
    next_order: Cell<u64>,
}

/// Where `BackHandler`s register, as AndroidX's `OnBackPressedDispatcher`; owned by the host.
#[derive(Clone, Default)]
pub struct OnBackPressedDispatcher {
    inner: Rc<DispatcherInner>,
}

impl compose_runtime::ComposeArg for OnBackPressedDispatcher {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl OnBackPressedDispatcher {
    pub fn new() -> OnBackPressedDispatcher {
        OnBackPressedDispatcher::default()
    }

    /// Whether a handler is enabled: a back press would go to it, so the platform intercepts it.
    pub fn has_enabled_callbacks(&self) -> bool {
        self.inner.callbacks.borrow().iter().any(|callback| callback.enabled.get())
    }

    /// Calls the innermost enabled handler, as `onBackPressed`. Returns whether one was.
    pub fn on_back_pressed(&self) -> bool {
        let handler = self
            .inner
            .callbacks
            .borrow()
            .iter()
            .filter(|callback| callback.enabled.get())
            .max_by_key(|callback| (callback.layer, callback.order))
            .map(|callback| callback.on_back.borrow().clone());
        // Called without the borrow: it may compose away handlers (it writes state).
        match handler {
            Some(handler) => {
                handler();
                true
            }
            None => false,
        }
    }

    fn add(&self, callback: &Rc<BackCallback>) {
        self.inner.callbacks.borrow_mut().push(callback.clone());
    }

    fn remove(&self, callback: &Rc<BackCallback>) {
        self.inner.callbacks.borrow_mut().retain(|other| !Rc::ptr_eq(other, callback));
    }

    fn next_order(&self) -> u64 {
        let order = self.inner.next_order.get();
        self.inner.next_order.set(order + 1);
        order
    }
}

/// The dispatcher `BackHandler` registers with, as Compose's `LocalOnBackPressedDispatcherOwner`:
/// the host's, or none (a handler then does nothing).
pub static LOCAL_ON_BACK_PRESSED_DISPATCHER: CompositionLocal<Option<OnBackPressedDispatcher>> =
    CompositionLocal::new("LocalOnBackPressedDispatcher", || None);

/// How many dialogs or focusable popups the content is in: their handlers go first.
pub(crate) static LOCAL_BACK_LAYER: CompositionLocal<u32> =
    CompositionLocal::new("LocalBackLayer", || 0);

/// Registered while in the composition.
struct BackRegistration {
    dispatcher: Option<OnBackPressedDispatcher>,
    callback: Rc<BackCallback>,
}

impl RememberObserver for BackRegistration {
    fn on_remembered(&self) {
        if let Some(dispatcher) = &self.dispatcher {
            dispatcher.add(&self.callback);
        }
    }

    fn on_forgotten(&self) {
        if let Some(dispatcher) = &self.dispatcher {
            dispatcher.remove(&self.callback);
        }
    }
}

/// Calls `on_back` when the user goes back (Android's back gesture or button, Escape on
/// desktop) while `enabled`, as AndroidX's `BackHandler`: the innermost enabled handler gets
/// it (those in dialogs and focusable popups first, then the one composed last). While any is
/// enabled, back doesn't leave the app.
#[composable]
#[allow(non_snake_case)]
pub fn BackHandler(#[default(true)] enabled: bool, on_back: impl Fn() + 'static) {
    let registration = remember_observer(|| {
        let dispatcher = LOCAL_ON_BACK_PRESSED_DISPATCHER.current();
        let layer = LOCAL_BACK_LAYER.current();
        let order = dispatcher.as_ref().map_or(0, OnBackPressedDispatcher::next_order);
        BackRegistration {
            dispatcher,
            callback: Rc::new(BackCallback {
                enabled: Cell::new(enabled),
                layer,
                order,
                on_back: RefCell::new(on_back.clone()),
            }),
        }
    });
    let callback = registration.callback.clone();
    // Once the pass is applied, as Compose's `SideEffect` updating the callback.
    SideEffect(move || {
        callback.enabled.set(enabled);
        *callback.on_back.borrow_mut() = on_back;
    });
}
