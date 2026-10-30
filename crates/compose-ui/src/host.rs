//! `UiHost`: a composition of a UI tree, with its frame loop and event entry points. Platform
//! hosts (desktop, Android) wrap it; tests drive it headless (docs/07-platforms.md, "The frame
//! loop").

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_lifecycle::{LifecycleOwner, LifecycleRegistry, LifecycleState, ViewModelStore};
use compose_runtime::Composition;

use crate::display_list::FlatDisplayList;
use crate::fixed_text::FixedTextMeasurer;
use crate::input::{Action, KeyEvent, PointerEvent};
use crate::semantics::SemanticsSnapshot;
use crate::text::TextMeasurer;
use crate::tree::{NodeId, UiTree};
use crate::unit::{Density, IntSize};

/// What a frame did.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FrameStats {
    pub recomposed: bool,
    pub measured: bool,
    pub drew: bool,
    /// A modifier node's semantics changed without a recomposition or a layout saying so
    /// (`invalidate_semantics`, or a semantics node's update): accessibility reads them again.
    pub semantics_changed: bool,
    /// Composable bodies that ran (didn't skip).
    pub bodies_run: u64,
}

/// What the host calls when it needs a frame, set by the platform.
type Waker = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// A composition of a UI tree in a root of a given size: the core every host shares.
pub struct UiHost {
    composition: Composition,
    /// After `composition`: dropped after it, as a window's lifecycle ends once its composition is
    /// disposed.
    /// Its tree's key: what's asked of the focus finds the tree if it's free through it.
    tree_key: u32,
    lifecycle: HostLifecycle,
    size: IntSize,
    frame_requested: Rc<Cell<bool>>,
    /// A frame is running: what its tasks write is drawn by it.
    framing: Rc<Cell<bool>>,
    /// Content to compose at the next frame.
    pending_content: RefCell<Option<Box<dyn Fn()>>>,
    waker: Waker,
    /// The density content sees as `LOCAL_DENSITY`.
    density: compose_runtime::OwnedState<Density>,
    /// The view configuration the platform set, if any (`LOCAL_VIEW_CONFIGURATION`; else
    /// Android's at the density).
    view_configuration:
        compose_runtime::OwnedState<Option<crate::view_configuration::ViewConfiguration>>,
    /// The layout direction content sees as `LOCAL_LAYOUT_DIRECTION`.
    layout_direction: compose_runtime::OwnedState<crate::alignment::LayoutDirection>,
    /// The window's size, the root's constraints hint (`LOCAL_CONSTRAINTS_HINT`).
    window_size: compose_runtime::OwnedState<IntSize>,
    /// Work lazy lists ask for between frames (`prefetch`), given to content as
    /// `LOCAL_PREFETCH_SCHEDULER`.
    prefetch_scheduler: crate::prefetch::PrefetchScheduler,
    /// Where `BackHandler`s register (`LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER`).
    navigation_events: compose_navigation_event::NavigationEventDispatcher,
    /// How back reaches them (`LOCAL_ON_BACK_PRESSED_DISPATCHER`).
    back_dispatcher: crate::back::OnBackPressedDispatcher,
    /// Where `remember_saveable` registers (`LOCAL_SAVEABLE_STATE_REGISTRY`).
    saveable_registry: compose_runtime::SaveableStateRegistry,
    /// The platform's configuration (`LOCAL_CONFIGURATION`).
    configuration: compose_runtime::OwnedState<crate::configuration::Configuration>,
    /// The window's insets, a state per type once read (`LOCAL_WINDOW_INSETS_SOURCE`).
    window_insets: Rc<crate::configuration::HostInsets>,
    /// The platform's properties and its event listener (`LOCAL_HOST_PROPS`,
    /// `LOCAL_HOST_EVENTS`).
    props: crate::configuration::HostProps,
    events: crate::configuration::HostEvents,
}

/// The content, skipped when the root recomposes for a new density: only what reads
/// `LOCAL_DENSITY` runs again.
#[compose_runtime::composable]
#[allow(non_snake_case)]
fn Content(content: compose_runtime::Callback) {
    content()
}

/// The constraints what's composed here will likely be measured with, for content that composes
/// for its constraints (`BoxWithConstraints`) to compose right the first time: the window's at
/// the root, the items' in a lazy list, the cells' in a grid. Compose composes such content
/// while measuring; here it composes again within the frame when the hint was wrong.
pub static LOCAL_CONSTRAINTS_HINT: compose_runtime::CompositionLocal<
    Option<crate::constraints::Constraints>,
> = compose_runtime::CompositionLocal::new("LocalConstraintsHint", || None);

/// The window's density, as Compose's `LocalDensity`: what composition converts `Dp` with.
pub static LOCAL_DENSITY: compose_runtime::CompositionLocal<Density> =
    compose_runtime::CompositionLocal::new_static("LocalDensity", Density::default);

/// The most times a frame composes and lays out again for what its layout asked (D487).
const MOST_ROUNDS: usize = 16;

impl UiHost {
    /// A host of `size` pixels at `density`, measuring text with `text_measurer`.
    pub fn new(size: IntSize, density: Density, text_measurer: Box<dyn TextMeasurer>) -> UiHost {
        let composition = Composition::new(UiTree::new(density, text_measurer));
        let frame_requested = Rc::new(Cell::new(true));
        let framing = Rc::new(Cell::new(false));
        let waker: Waker = Rc::new(RefCell::new(None));
        // A write asks for a frame; one made while a frame runs its tasks is that frame's (an
        // animation's step), so it wakes nothing.
        let request = {
            let (requested, framing, wake) =
                (frame_requested.clone(), framing.clone(), waker.clone());
            Rc::new(move || {
                requested.set(true);
                if framing.get() {
                    return;
                }
                let waker = wake.borrow().clone();
                if let Some(waker) = waker {
                    waker();
                }
            })
        };
        // An enabled back handler changing outside a frame (a lifecycle's start, a task): the platform
        // learns of it from a frame. One changing in a frame is told by that frame.
        let handlers_changed = {
            let (requested, framing, wake) =
                (frame_requested.clone(), framing.clone(), waker.clone());
            move || {
                if framing.get() {
                    return;
                }
                requested.set(true);
                let waker = wake.borrow().clone();
                if let Some(waker) = waker {
                    waker();
                }
            }
        };
        let requester = request.clone();
        composition.set_frame_requester(move || requester());
        // `FocusRequester`s ask for a frame, which moves the focus.
        let focus_wake = request.clone();
        composition.with_applier(|tree: &mut UiTree| {
            *tree.focus_requests.wake.borrow_mut() = Some(Rc::new(move || focus_wake()));
        });
        // Modifier nodes' handles reach the tree through the composition, when it's free.
        let applier = composition.applier_ref::<UiTree>();
        let key = composition.with_applier(|tree: &mut UiTree| tree.key);
        crate::node_context::set_tree_borrow(
            key,
            Rc::new(move |f: &mut dyn FnMut(&mut UiTree)| {
                applier.try_with(|tree| f(tree)).is_some()
            }),
        );
        // Writes to state read while measuring or drawing ask for a frame, as composition's do.
        let observer = compose_runtime::platform::ReadObserver::new(move || request());
        composition.with_applier(|tree: &mut UiTree| tree.set_read_observer(observer));
        let saveable_registry = compose_runtime::SaveableStateRegistry::new(None);
        let window_insets = Rc::new(crate::configuration::HostInsets::new());
        let insets_source = crate::configuration::WindowInsetsSource::of(window_insets.clone());
        composition.with_applier(|tree: &mut UiTree| tree.set_window_insets_source(insets_source));
        let navigation_events = compose_navigation_event::NavigationEventDispatcher::new();
        let tree_key = composition.with_applier(|tree: &mut UiTree| tree.key);
        UiHost {
            tree_key,
            composition: {
                composition.with_applier(|tree: &mut UiTree| {
                    tree.saveable_registry = Some(saveable_registry.clone());
                });
                composition
            },
            lifecycle: HostLifecycle::new(),
            size,
            frame_requested,
            framing,
            pending_content: RefCell::new(None),
            waker,
            density: compose_runtime::mutable_state_of(density),
            view_configuration: compose_runtime::mutable_state_of(None),
            layout_direction: compose_runtime::mutable_state_of(
                crate::alignment::LayoutDirection::Ltr,
            ),
            window_size: compose_runtime::mutable_state_of(size),
            prefetch_scheduler: crate::prefetch::PrefetchScheduler::default(),
            navigation_events: navigation_events.clone(),
            back_dispatcher: crate::back::OnBackPressedDispatcher::new(
                &navigation_events,
                handlers_changed,
            ),
            saveable_registry,
            configuration: compose_runtime::mutable_state_of(
                crate::configuration::Configuration::from_window(size, density, false),
            ),
            window_insets,
            props: crate::configuration::HostProps::default(),
            events: crate::configuration::HostEvents::default(),
        }
    }

    /// A headless host at density 1 with the fixed-advance text measurer: for tests.
    pub fn headless(width: i32, height: i32) -> UiHost {
        UiHost::new(
            IntSize::new(width, height),
            Density::default(),
            Box::new(FixedTextMeasurer::new()),
        )
    }

    /// Composes `content` at the next frame, as Compose's `setContent`: a `LaunchedEffect` it
    /// starts then waits for the frame after.
    pub fn set_content(&self, content: impl Fn() + 'static) {
        *self.pending_content.borrow_mut() = Some(Box::new(content));
        self.wake();
    }

    /// Asks for a frame: the platform's waker schedules one.
    fn wake(&self) {
        self.frame_requested.set(true);
        let waker = self.waker.borrow().clone();
        if let Some(waker) = waker {
            waker();
        }
    }

    /// After dispatch: wakes the platform if the tree changed (focus, a modifier node's state).
    fn wake_if_dirty(&self) {
        self.handle_focus_requests();
        if self.with_tree(|tree| tree.needs_draw || tree.semantics_changed) {
            self.wake();
        }
    }

    /// Runs the side effects modifier nodes asked for (`NodeContext::side_effect`), now that the
    /// tree is free, as Compose's `onEndApplyChanges`: what they queue in turn runs too.
    fn run_side_effects(&self) {
        loop {
            let effects = self.with_tree(|tree| {
                let mut effects = std::mem::take(&mut tree.side_effects);
                effects.append(&mut tree.focus_requests.side_effects.borrow_mut());
                effects
            });
            if effects.is_empty() {
                return;
            }
            for effect in effects {
                effect();
            }
        }
    }

    /// Moves focus as `FocusRequester`s asked, now that the tree is free, running what the moves
    /// call (`on_focus_changed`) without recomposing: a frame does it first, and a test rule
    /// between frames, so what the calls write recomposes at its next frame (D906).
    pub fn handle_focus_requests(&self) {
        let mut _turn = None;
        loop {
            let pending = self.with_tree(|tree| tree.focus_requests.pending.take());
            if pending.is_empty() {
                return;
            }
            _turn.get_or_insert_with(compose_runtime::platform::Turn::begin);
            for request in pending {
                let actions = self.with_tree(|tree| tree.perform_focus_request(request));
                Self::run(actions);
            }
        }
    }

    /// Called when the host needs a frame, for a platform host to schedule one: when a send
    /// invalidates what state read (at the end of the turn that wrote it; a write outside any
    /// turn first asks for a turn through the executor's wake handler, D755), and when the host's
    /// own properties change.
    pub fn set_frame_waker(&self, waker: impl Fn() + 'static) {
        *self.waker.borrow_mut() = Some(Rc::new(waker));
    }

    pub fn composition(&self) -> &Composition {
        &self.composition
    }

    pub fn size(&self) -> IntSize {
        self.size
    }

    pub fn resize(&mut self, size: IntSize) {
        let _turn = compose_runtime::platform::Turn::begin();
        if self.size != size {
            self.size = size;
            self.window_size.set(size);
            self.wake();
        }
    }

    /// The host's lifecycle, what `LOCAL_LIFECYCLE_OWNER` gives its content.
    pub fn lifecycle(&self) -> compose_lifecycle::Lifecycle {
        self.lifecycle.registry.lifecycle()
    }

    /// Moves the host's lifecycle, as its platform's does (an activity's, a window's focus). Once
    /// destroyed, it stays so. `Initialized` is `Created`: a lifecycle can't go back to it.
    pub fn set_lifecycle_state(&self, state: LifecycleState) {
        let _turn = compose_runtime::platform::Turn::begin();
        let registry = &self.lifecycle.registry;
        if registry.current_state() != LifecycleState::Destroyed {
            registry.set_current_state(if state == LifecycleState::Initialized {
                LifecycleState::Created
            } else {
                state
            });
        }
    }

    /// The store of view models its content gets by default, as an activity's.
    pub fn view_model_store(&self) -> ViewModelStore {
        self.lifecycle.view_models.clone()
    }

    /// Makes `store` that store, before the content, kept by the caller when the host goes: what a
    /// platform does to keep view models across a configuration change (D838).
    pub fn set_retained_view_model_store(&mut self, store: ViewModelStore) {
        self.lifecycle.view_models = store;
        self.lifecycle.clears_view_models = false;
    }

    /// Lays the content out in `direction` (the window's: right-to-left for Arabic or Hebrew
    /// locales), as `LocalLayoutDirection` at the root.
    pub fn set_layout_direction(&mut self, direction: crate::alignment::LayoutDirection) {
        let _turn = compose_runtime::platform::Turn::begin();
        self.layout_direction.set(direction);
        self.with_tree(|tree| {
            let root = tree.root_id();
            tree.set_layout_direction(root, direction);
        });
        self.wake();
    }

    /// The gesture timeouts and distances the detectors use and content reads as
    /// `LOCAL_VIEW_CONFIGURATION`, as a platform's `ViewConfiguration`; `None` (the default):
    /// Android's at the density (D907).
    pub fn set_view_configuration(
        &mut self,
        configuration: Option<crate::view_configuration::ViewConfiguration>,
    ) {
        let _turn = compose_runtime::platform::Turn::begin();
        self.with_tree(|tree| tree.set_view_configuration(configuration));
        self.view_configuration.set(configuration);
        self.wake();
    }

    /// The view configuration the detectors use now.
    pub fn view_configuration(&self) -> crate::view_configuration::ViewConfiguration {
        self.with_tree(|tree| tree.view_configuration())
    }

    pub fn set_density(&mut self, density: Density) {
        let _turn = compose_runtime::platform::Turn::begin();
        self.with_tree(|tree| tree.set_density(density));
        self.density.set(density);
        self.wake();
    }

    /// The platform's configuration, which content reads as `LOCAL_CONFIGURATION` (on desktop,
    /// the window's size in dp and the system's dark mode).
    pub fn set_configuration(&mut self, configuration: crate::configuration::Configuration) {
        let _turn = compose_runtime::platform::Turn::begin();
        self.configuration.set(configuration);
    }

    pub fn configuration(&self) -> crate::configuration::Configuration {
        self.configuration.handle().get()
    }

    /// The window's insets (system bars, the keyboard, a cutout, …), which foundation's
    /// `WindowInsets` read (through `LOCAL_WINDOW_INSETS_SOURCE`), each type's on its own.
    pub fn set_window_insets(&mut self, insets: crate::configuration::PlatformWindowInsets) {
        let _turn = compose_runtime::platform::Turn::begin();
        self.window_insets.report(insets);
    }

    /// The display cutout's outline, if the platform reports one, which foundation's
    /// `WindowInsets::cutout_path` reads.
    pub fn set_window_cutout_path(&mut self, path: Option<crate::path::Path>) {
        let _turn = compose_runtime::platform::Turn::begin();
        self.window_insets.report_cutout_path(path);
    }

    /// The properties the platform passes to the content (`LOCAL_HOST_PROPS`).
    pub fn props(&self) -> &crate::configuration::HostProps {
        &self.props
    }

    /// Called with the events the content sends the platform (`LOCAL_HOST_EVENTS`): a name and
    /// a payload.
    pub fn set_event_listener(&self, listener: impl Fn(&str, &str) + 'static) {
        self.events.set_listener(Some(Rc::new(listener)));
    }

    /// The user went back (Android's back gesture or button): the innermost enabled
    /// `BackHandler` gets it. Returns whether one did; if none, the platform goes back itself
    /// (Android's activity finishes).
    pub fn on_back_pressed(&mut self) -> bool {
        let _turn = compose_runtime::platform::Turn::begin();
        let handled = self.back_dispatcher.on_back_pressed();
        self.wake_if_dirty();
        handled
    }

    /// A back gesture starts (Android's, API 34 and up): the innermost enabled handler takes it, to
    /// the end, which is [`on_back_pressed`](Self::on_back_pressed) or
    /// [`on_back_cancelled`](Self::on_back_cancelled).
    pub fn on_back_started(&mut self, event: crate::back::BackEventCompat) {
        let _turn = compose_runtime::platform::Turn::begin();
        self.back_dispatcher.dispatch_on_back_started(event);
        self.wake_if_dirty();
    }

    /// It goes on.
    pub fn on_back_progressed(&mut self, event: crate::back::BackEventCompat) {
        let _turn = compose_runtime::platform::Turn::begin();
        self.back_dispatcher.dispatch_on_back_progressed(event);
        self.wake_if_dirty();
    }

    /// It's given up.
    pub fn on_back_cancelled(&mut self) {
        let _turn = compose_runtime::platform::Turn::begin();
        self.back_dispatcher.dispatch_on_back_cancelled();
        self.wake_if_dirty();
    }

    /// Whether a `BackHandler` is enabled: the platform intercepts back only then. Platforms ask
    /// after each frame, and a change outside one asks for a frame.
    pub fn has_enabled_back_handlers(&self) -> bool {
        self.back_dispatcher.has_enabled_callbacks()
    }

    /// The values `remember_saveable` registered, for the platform to keep while the host is
    /// recreated (an Android configuration change, the process killed in the background):
    /// `SavedState::to_bytes` makes the bytes to keep.
    pub fn save_state(&self) -> compose_runtime::SavedState {
        self.saveable_registry.perform_save()
    }

    /// Gives `state` (what an earlier host saved) back to the `remember_saveable` calls that
    /// compose from now on: call it before the first frame.
    pub fn restore_state(&self, state: compose_runtime::SavedState) {
        self.saveable_registry.restore(state);
    }

    fn focus_requests_of_host(&self) -> Rc<crate::focus_requester::FocusRequests> {
        crate::node_context::requests_of(self.tree_key).expect("compose-ui: a host's tree")
    }

    /// Runs `f` with the tree.
    pub fn with_tree<R>(&self, f: impl FnOnce(&mut UiTree) -> R) -> R {
        self.composition.with_applier(f)
    }

    /// Whether a frame would do anything.
    pub fn needs_frame(&self) -> bool {
        self.has_pending_work() || compose_runtime::platform::has_frame_awaiters()
    }

    /// Whether a frame would do anything besides resuming the tasks waiting for one: state
    /// changed, something to draw, a task ready.
    pub fn has_pending_work(&self) -> bool {
        // Writes not sent yet ask for a frame once sent (D751).
        compose_runtime::snapshots::Snapshot::send_apply_notifications();
        self.frame_requested.get()
            || self.pending_content.borrow().is_some()
            || self.composition.has_invalidations()
            // A frame tells a change of the semantics, drawn or not (D1263, D1266).
            || self.with_tree(|tree| tree.needs_draw || tree.semantics_changed)
            || compose_runtime::platform::has_ready_tasks()
            || self.with_tree(|tree| !tree.focus_requests.pending.borrow().is_empty())
            || self.with_tree(|tree| {
                tree.has_node_requests()
                    || !tree.side_effects.is_empty()
                    || !tree.focus_requests.side_effects.borrow().is_empty()
            })
            || self.with_tree(|tree| tree.has_beyond_bounds_moves())
    }

    /// One frame: runs the tasks that are due and resumes those waiting for a frame (with this
    /// frame's time), recomposes invalid scopes, measures and draws what changed, then starts the
    /// tasks the pass launched.
    pub fn frame(&mut self) -> FrameStats {
        let _turn = compose_runtime::platform::Turn::begin();
        // The tasks due and those waiting for this frame write state this frame recomposes,
        // measures and draws: they ask for no other frame.
        self.framing.set(true);
        let tasks = crate::trace::section(c"compose-rs: tasks");
        compose_runtime::platform::fire_timers();
        compose_runtime::platform::run_ready();
        self.handle_focus_requests();
        // What the tasks' modifier node handles asked, in this frame.
        self.with_tree(|tree| tree.apply_node_requests());
        self.run_side_effects();
        compose_runtime::platform::dispatch_frame();
        drop(tasks);
        let compose = crate::trace::section(c"compose-rs: recompose");
        self.frame_requested.set(false);
        let bodies = compose_runtime::platform::bodies_run();
        let content = self.pending_content.borrow_mut().take();
        let composed = content.is_some();
        if let Some(content) = content {
            let density = self.density.handle();
            let view_configuration = self.view_configuration.handle();
            let direction = self.layout_direction.handle();
            let window = self.window_size.handle();
            let constants = HostLocals {
                lifecycle: self.lifecycle.registry.lifecycle(),
                view_models: Rc::new(self.lifecycle.view_models.clone()),
                scheduler: self.prefetch_scheduler.clone(),
                navigation_events: self.navigation_events.clone(),
                back_dispatcher: self.back_dispatcher.clone(),
                saveable_registry: self.saveable_registry.clone(),
                props: self.props.clone(),
                events: self.events.clone(),
                insets: crate::configuration::WindowInsetsSource::of(self.window_insets.clone()),
                window_info: crate::focus_manager::WindowInfo::main(
                    self.with_tree(|tree| Rc::downgrade(&tree.focus_requests)),
                ),
            };
            let configuration = self.configuration.handle();
            let content: compose_runtime::Callback = compose_runtime::callback(content);
            self.composition.set_content(move || {
                let content = content.clone();
                // What never changes, outermost: read without recomposing anything.
                constants.provide(move || {
                    let content = content.clone();
                    compose_runtime::CompositionLocalProvider(
                        LOCAL_DENSITY.provides(density.get()),
                        move || {
                            let gestures = view_configuration.get().unwrap_or_else(|| {
                                crate::view_configuration::ViewConfiguration::at(density.get())
                            });
                            compose_runtime::CompositionLocalProvider(
                                crate::view_configuration::LOCAL_VIEW_CONFIGURATION
                                    .provides(gestures),
                                move || {
                                    compose_runtime::CompositionLocalProvider(
                                        crate::alignment::LOCAL_LAYOUT_DIRECTION
                                            .provides(direction.get()),
                                        move || {
                                            let size = window.get();
                                            compose_runtime::CompositionLocalProvider(
                                                LOCAL_CONSTRAINTS_HINT.provides(Some(
                                                    crate::constraints::Constraints::fixed(
                                                        size.width,
                                                        size.height,
                                                    ),
                                                )),
                                                move || {
                                                    compose_runtime::CompositionLocalProvider(
                                                        crate::configuration::LOCAL_CONFIGURATION
                                                            .provides(configuration.get()),
                                                        move || Content(content),
                                                    )
                                                },
                                            )
                                        },
                                    )
                                },
                            )
                        },
                    )
                })
            });
        }
        let mut recomposed =
            (self.composition.has_invalidations() && self.composition.recompose()) || composed;
        // The pass's nodes' side effects, before the layout reads what they write (D1161).
        self.run_side_effects();
        drop(compose);
        let size = self.size;
        let invalid_before_layout = self.composition.has_invalidations();
        // A write the pass made to state read while measuring or drawing (a transition's target,
        // written as it composes, D802) is this frame's layout's and drawing's (`lay_out` applies
        // it first): it asks for no other frame. One invalidating composition still does.
        if !invalid_before_layout {
            self.frame_requested.set(false);
        }
        let layout = crate::trace::section(c"compose-rs: layout");
        let mut measured = self.lay_out(size);
        drop(layout);
        // A layout that changed what to compose (a lazy list reaching items it lacks) is
        // composed and laid out again in this frame, `MOST_ROUNDS` times at most, before
        // drawing: a list asks for items geometrically, so a frame filling a phone's screen
        // with rows a few pixels tall takes about 8 (D487).
        let mut rounds = 0;
        while !invalid_before_layout && rounds < MOST_ROUNDS && self.composition.has_invalidations()
        {
            rounds += 1;
            let _round = crate::trace::section(c"compose-rs: recompose and layout again");
            // The writes that asked for this round are composed in it: they ask for no other
            // frame (which also held prefetch back in the idle time after this one, so the next
            // frame composed items itself again, D474).
            self.frame_requested.set(false);
            recomposed |= self.composition.recompose();
            self.run_side_effects();
            measured |= self.lay_out(size);
        }
        // What the rounds left (they ran out) is the next frame's.
        if rounds > 0 && self.composition.has_invalidations() {
            self.frame_requested.set(true);
        }
        // Focus moves waiting for a lazy list's items beyond its bounds, laid out now; each may
        // ask for one more item, laid out (and composed) here too, a few times a frame (D477).
        // Laid out again after each, so the item the focus found is shown (scrolled into view)
        // in this frame too (D485).
        let mut searches = 0;
        while searches < 8 && self.with_tree(|tree| tree.has_beyond_bounds_moves()) {
            searches += 1;
            let actions = self.with_tree(|tree| tree.run_beyond_bounds_moves());
            Self::run(actions);
            measured |= self.lay_out(size);
            let mut rounds = 0;
            while rounds < MOST_ROUNDS && self.composition.has_invalidations() {
                rounds += 1;
                let _round = crate::trace::section(c"compose-rs: recompose and layout again");
                // As the frame's rounds (D474): what asked for this one is composed in it.
                self.frame_requested.set(false);
                recomposed |= self.composition.recompose();
                self.run_side_effects();
                measured |= self.lay_out(size);
            }
            if self.composition.has_invalidations()
                || self.with_tree(|tree| tree.has_beyond_bounds_moves())
            {
                self.frame_requested.set(true);
            }
        }
        // A node focused before it was placed is shown, now that it is.
        let shown = self.with_tree(|tree| {
            let node = tree.bring_into_view_after_layout.take()?;
            tree.get(node)?;
            if !tree.is_laid_out(node) {
                return None;
            }
            tree.bring_into_view(node);
            Some(())
        });
        if shown.is_some() {
            measured |= self.lay_out(size);
        }
        // Content that moved under a resting mouse: the hover follows it, as Compose Desktop's
        // synthetic move after a layout.
        // Pointer nodes that came, went or changed without a layout too (D1159).
        let hover_stale = self.with_tree(|tree| std::mem::take(&mut tree.hover_stale));
        if measured || hover_stale {
            let actions = self.with_tree(|tree| tree.refresh_hover());
            Self::run(actions);
            // A hover it found draws in this frame (D1053), asked through a handle or not.
            self.with_tree(|tree| tree.apply_node_requests());
        }
        let draw = crate::trace::section(c"compose-rs: draw");
        // Layout's and its callbacks' writes reach the drawing (D751).
        compose_runtime::snapshots::Snapshot::send_apply_notifications();
        let (drew, actions) = self.with_tree(|tree| (tree.draw(), tree.take_pending_actions()));
        drop(draw);
        // What composition queued (a focus lost, listeners attached), now that the tree is free.
        Self::run(actions);
        // `LaunchedEffect`s started by the pass take their first step.
        compose_runtime::platform::run_ready();
        // What the pass and its effects wrote needs the next frame.
        self.framing.set(false);
        if self.frame_requested.get() {
            self.wake();
        }
        let semantics_changed = self.with_tree(|tree| std::mem::take(&mut tree.semantics_changed));
        FrameStats {
            recomposed,
            measured,
            drew,
            semantics_changed,
            bodies_run: compose_runtime::platform::bodies_run() - bodies,
        }
    }

    /// Frames until nothing is left to do (at most `limit`): effects writing state get their
    /// recomposition. Returns the number of frames run.
    pub fn settle(&mut self, limit: usize) -> usize {
        let mut frames = 0;
        while frames < limit && self.needs_frame() {
            self.frame();
            frames += 1;
        }
        frames
    }

    /// Whether prefetch requests wait for idle time: the platform then calls `prefetch` after
    /// the frame.
    pub fn has_prefetch_work(&self) -> bool {
        self.prefetch_scheduler.has_requests()
    }

    /// Runs the prefetch requests until `deadline` (when the next frame should start), as
    /// Compose's `PrefetchScheduler`: a lazy list that scrolled composes and measures the items
    /// it will reach next, so the frame that scrolls them in finds them ready. The platform calls
    /// it when idle between frames. Returns whether work is left, for the next idle time.
    ///
    /// What it composes isn't placed until a frame wants it: nothing shows, state stays as it was
    /// (a list's first visible item), and no frame is asked for. A frame with work waiting (state
    /// changed, a scroll) goes first: nothing runs then.
    pub fn prefetch(&mut self, deadline: std::time::Instant) -> bool {
        self.prefetch_with(deadline, false)
    }

    /// [`prefetch`](UiHost::prefetch), the first request without a deadline when
    /// `unbounded_first`: as Compose's `AndroidPrefetchScheduler` when no frame was drawn for
    /// two intervals (`isFrameIdle`), the others until `deadline`.
    pub fn prefetch_with(&mut self, deadline: std::time::Instant, unbounded_first: bool) -> bool {
        if !self.has_prefetch_work() {
            return false;
        }
        if self.has_pending_work() {
            return true;
        }
        let _turn = compose_runtime::platform::Turn::begin();
        let _prefetch = crate::trace::section(c"compose-rs: prefetch");
        // The requests' writes are composed here: they wake nothing.
        self.framing.set(true);
        let back_handlers = self.has_enabled_back_handlers();
        let more = self.prefetch_scheduler.run(&self.composition, deadline, unbounded_first);
        self.run_side_effects();
        // The items composed aren't placed, and those disposed weren't: unless measuring wrote
        // state for others (observed writes, scopes invalidated), nothing needs a frame. Each
        // item is a repaint boundary, so measuring one marks only its own drawing (D155), drawn
        // when a frame places it; the boundary around the list, if any, is marked as the list
        // is (its children changed), and drawn again with the list's next measurement.
        // The requests' writes invalidate what read them (D751).
        compose_runtime::snapshots::Snapshot::send_apply_notifications();
        let quiet = self.with_tree(|tree| {
            tree.needs_draw = false;
            tree.apply_observed_writes();
            // A frame tells a change of the semantics, drawn or not.
            !tree.needs_draw && !tree.semantics_changed
        });
        self.framing.set(false);
        // An item's back handler: no frame's here to tell the platform.
        let back_handlers_same = back_handlers == self.has_enabled_back_handlers();
        if quiet && back_handlers_same && !self.composition.has_invalidations() {
            self.frame_requested.set(false);
        } else {
            self.with_tree(|tree| tree.needs_draw = true);
            self.wake();
        }
        // What composition queued (listeners attached), and the effects it started.
        let actions = self.with_tree(|tree| tree.take_pending_actions());
        Self::run(actions);
        compose_runtime::platform::run_ready();
        self.wake_if_dirty();
        more
    }

    /// Whether an item `prefetch` measured waits to be drawn ahead ([`draw_ahead`]).
    ///
    /// [`draw_ahead`]: UiHost::draw_ahead
    pub fn has_draw_ahead_work(&self) -> bool {
        self.with_tree(|tree| tree.has_draw_ahead_work())
    }

    /// Whether each frame lists the groups of the repaint boundaries composed and no longer
    /// placed (`DisplayList::kept`, D380): for a platform keeping a recording per group
    /// (Android's render nodes). Off by default: a renderer drawing the tree has no use for them.
    pub fn set_keep_unplaced_groups(&mut self, keep: bool) {
        self.with_tree(|tree| tree.set_keep_unplaced_groups(keep));
    }

    /// Uses `optimizations`, compose-rs's own beyond Compose's rendering (D520): the renderer
    /// reads them ([`optimizations`](UiHost::optimizations)), and so does the tree. The next
    /// frame draws everything again if they changed.
    pub fn set_optimizations(&mut self, optimizations: crate::optimizations::Optimizations) {
        self.with_tree(|tree| tree.set_optimizations(optimizations));
        self.wake();
    }

    /// The optimizations in use ([`set_optimizations`](UiHost::set_optimizations); by default
    /// `Optimizations::defaults()`, the recommended set: D610).
    pub fn optimizations(&self) -> crate::optimizations::Optimizations {
        self.with_tree(|tree| tree.optimizations())
    }

    /// Gives up drawing ahead the items waiting ([`UiTree::cancel_draw_ahead`]): the frames
    /// placing them draw them.
    pub fn cancel_draw_ahead(&mut self) {
        self.with_tree(|tree| tree.cancel_draw_ahead());
    }

    /// Draws ahead, between frames, the oldest repaint boundary `prefetch` measured and no frame
    /// placed yet (a lazy list's next item): its group, then the groups nested in it, which the
    /// frame placing it shares, versions unchanged ([`UiTree::draw_ahead`]). For a platform
    /// recording each group (Android's render nodes): it records them now, and that frame
    /// records nothing new. `None` without such a boundary, or when a frame has work (it goes
    /// first, as for `prefetch`).
    ///
    /// Drawing reads state observed as in a frame: a later write draws the boundary again (and a
    /// write while drawing asks for a frame, as any).
    pub fn draw_ahead(&mut self) -> Option<Vec<crate::display_list::DrawGroup>> {
        if self.has_pending_work() {
            return None;
        }
        let _turn = compose_runtime::platform::Turn::begin();
        let _section = crate::trace::section(c"compose-rs: draw ahead");
        compose_runtime::snapshots::Snapshot::send_apply_notifications();
        self.with_tree(|tree| tree.draw_ahead())
    }

    fn run(actions: Vec<Action>) {
        crate::input::run_actions(actions);
    }

    /// Measures and places what the writes observed since invalidated, then runs the layout
    /// callbacks (`on_size_changed`, `on_globally_positioned`). A measurement reading state that
    /// another measurement or a callback wrote (a lazy list's viewport size, which its items'
    /// `fill_parent_max_*` read) is measured again within this layout, a few rounds at most,
    /// rather than at the next frame.
    fn lay_out(&mut self, size: IntSize) -> bool {
        let mut measured = false;
        // A callback's writes read by measurements are laid out here too (those read by
        // composition, by the frame's next round).
        for _ in 0..4 {
            // The writes so far invalidate what read them (D751), then the layout applies them.
            compose_runtime::snapshots::Snapshot::send_apply_notifications();
            let again = self.with_tree(|tree| {
                tree.apply_node_requests();
                tree.apply_observed_writes();
                tree.layout(size)
            });
            measured |= again;
            let (actions, told) = self.with_tree(|tree| {
                let actions = tree.take_layout_callbacks();
                (actions, std::mem::take(&mut tree.layout_nodes_told))
            });
            // A node told may have invalidated or written state too.
            let called = told || !actions.is_empty();
            Self::run(actions);
            if !again && !called {
                break;
            }
        }
        measured
    }

    /// Dispatches a pointer event (its position in the root), then runs the handlers' actions.
    pub fn send_pointer(&mut self, mut event: PointerEvent) {
        let _turn = compose_runtime::platform::Turn::begin();
        if event.time_nanos == 0 {
            event.time_nanos = compose_runtime::platform::now_nanos();
        }
        let actions = self.with_tree(|tree| tree.dispatch_pointer(event));
        Self::run(actions);
        self.wake_if_dirty();
    }

    /// The mouse pointer's look where it is, as `pointerHoverIcon` set it.
    pub fn pointer_icon(&self) -> crate::input::PointerIcon {
        self.with_tree(|tree| tree.pointer_icon())
    }

    /// The mouse left the window: what it was over hears `Exit` (hover states end).
    pub fn send_pointer_left(&mut self) {
        let _turn = compose_runtime::platform::Turn::begin();
        let actions = self.with_tree(|tree| tree.pointer_left());
        Self::run(actions);
        self.wake_if_dirty();
    }

    /// Dispatches a key event to the focused node; returns whether it was consumed. Escape
    /// that nothing focused takes goes back (`on_back_pressed`), as a dialog window takes it
    /// on desktop.
    pub fn send_key(&mut self, event: KeyEvent) -> bool {
        self.send_key_reporting(event).0
    }

    /// A key and the text it commits, as the hosts have both from the platform: the text goes to
    /// what has the focus unless a preview consumed the key, whatever consumed it after that,
    /// as a text field takes the key it's about to type. Returns whether either was taken.
    pub fn send_key_and_text(&mut self, event: KeyEvent, text: Option<&str>) -> bool {
        let (consumed, previewed) = self.send_key_reporting(event);
        match text {
            Some(text) if !previewed => self.send_text(text) || consumed,
            _ => consumed,
        }
    }

    /// Whether the key was consumed, and whether by a preview.
    fn send_key_reporting(&mut self, event: KeyEvent) -> (bool, bool) {
        let _turn = compose_runtime::platform::Turn::begin();
        let (consumed, previewed, actions) = self.with_tree(|tree| tree.dispatch_key(event));
        Self::run(actions);
        if !consumed
            && event.key == crate::input::Key::Escape
            && event.kind == crate::input::KeyEventType::KeyDown
        {
            return (self.on_back_pressed(), false);
        }
        self.wake_if_dirty();
        (consumed, previewed)
    }

    /// Dispatches committed text to the focused node; returns whether it was taken.
    pub fn send_text(&mut self, text: &str) -> bool {
        let _turn = compose_runtime::platform::Turn::begin();
        let (consumed, actions) = self.with_tree(|tree| tree.dispatch_text(text));
        Self::run(actions);
        self.wake_if_dirty();
        consumed
    }

    /// Dispatches an input method's composition to the focused node; returns whether it was
    /// taken.
    pub fn send_preedit(&mut self, text: &str, cursor: Option<(usize, usize)>) -> bool {
        let _turn = compose_runtime::platform::Turn::begin();
        let (consumed, actions) = self.with_tree(|tree| tree.dispatch_preedit(text, cursor));
        Self::run(actions);
        self.wake_if_dirty();
        consumed
    }

    /// Whether a handler asked for the input method to be reset since the last call: the
    /// platform host then resets it.
    pub fn take_input_method_reset(&self) -> bool {
        self.with_tree(|tree| std::mem::take(&mut tree.input_method_reset))
    }

    /// Where the focused node takes text, in the root's pixels: where an input method's
    /// candidate window goes.
    pub fn text_input_area(&self) -> Option<crate::unit::Rect> {
        self.with_tree(|tree| tree.text_input_area())
    }

    /// What the focused text field asks of an on-screen keyboard (its type, action key,
    /// capitalization, auto-correction), as Compose's `ImeOptions`; `None` while no focused node
    /// takes text. A platform reads them when the field takes the focus (Android, for its
    /// `EditorInfo` in `onCreateInputConnection`; desktop ignores them).
    pub fn text_input_options(&self) -> Option<crate::text_input::ImeOptions> {
        self.with_tree(|tree| tree.text_input_options())
    }

    /// The input method's action key (Android's `performEditorAction`): the focused field runs
    /// its `KeyboardActions`. Returns whether it was taken.
    pub fn perform_ime_action(&mut self, action: crate::text_input::ImeAction) -> bool {
        let _turn = compose_runtime::platform::Turn::begin();
        let (consumed, actions) = self.with_tree(|tree| tree.dispatch_ime_action(action));
        Self::run(actions);
        self.wake_if_dirty();
        consumed
    }

    /// What the content last asked of the on-screen keyboard since the last call
    /// (`SoftwareKeyboardController`, a text field's `Done`): show (`Some(true)`) or hide
    /// (`Some(false)`). The asking wakes the host; a platform with an on-screen keyboard takes
    /// it after each frame and event (Android shows it for the focused field or hides it).
    pub fn take_software_keyboard_request(&self) -> Option<bool> {
        self.with_tree(|tree| tree.focus_requests.software_keyboard.take())
    }

    /// The action `key` that `node` offers, from its own (unmerged) configuration, as Compose's
    /// accessibility reads it (`unmergedConfig`), with what its parts set (a text field's inner
    /// field).
    pub fn semantics_action<F: ?Sized + 'static>(
        &self,
        node: NodeId,
        key: &'static crate::semantics::SemanticsPropertyKey<
            crate::semantics::AccessibilityAction<F>,
        >,
    ) -> Option<Rc<F>>
    where
        crate::semantics::AccessibilityAction<F>: crate::semantics::SemanticsValue,
    {
        let config = self.with_tree(|tree| tree.unmerged_semantics_config(node))?;
        config.get(key)?.action.clone()
    }

    /// Whether a node's semantics changed since the last frame: the next frame's
    /// `FrameStats::semantics_changed` so far (a focus change between frames, D1259).
    pub fn semantics_changed_since_frame(&self) -> bool {
        self.with_tree(|tree| tree.semantics_changed)
    }

    /// Performs `node`'s action `key` (an accessibility service's click or focus), as Compose's
    /// `performSemanticsAction`: the action is called outside the tree, then the focus moves
    /// it asked for. Returns what the action did (`false` if the node offers none).
    pub fn perform_semantics_action(
        &mut self,
        node: NodeId,
        key: &'static crate::semantics::SemanticsPropertyKey<
            crate::semantics::AccessibilityAction<crate::semantics::ActionFn>,
        >,
    ) -> bool {
        self.perform_semantics_action_with(node, key, |action| action())
    }

    /// `perform_semantics_action` for an action with arguments: `perform` calls it (Compose's
    /// `performSemanticsAction(key) { action -> … }`).
    pub fn perform_semantics_action_with<F: ?Sized + 'static>(
        &mut self,
        node: NodeId,
        key: &'static crate::semantics::SemanticsPropertyKey<
            crate::semantics::AccessibilityAction<F>,
        >,
        perform: impl FnOnce(&F) -> bool,
    ) -> bool
    where
        crate::semantics::AccessibilityAction<F>: crate::semantics::SemanticsValue,
    {
        let _turn = compose_runtime::platform::Turn::begin();
        let Some(action) = self.semantics_action(node, key) else { return false };
        let done = perform(&action);
        self.handle_focus_requests();
        self.wake_if_dirty();
        done
    }

    /// The input mode, as `LOCAL_INPUT_MODE_MANAGER`'s `input_mode`: `Keyboard` unless the host
    /// says otherwise.
    pub fn input_mode(&self) -> crate::input_mode::InputMode {
        self.with_tree(|tree| tree.input_mode.input_mode())
    }

    /// Sets the input mode: a platform with a touch screen says when it's being used
    /// (`InputMode::Touch`) and when keys are (`InputMode::Keyboard`), as Android's touch mode.
    /// In touch mode, a focused target of `Focusability::SYSTEM_DEFINED` loses the focus.
    pub fn set_input_mode(&mut self, mode: crate::input_mode::InputMode) {
        let _turn = compose_runtime::platform::Turn::begin();
        self.with_tree(|tree| tree.input_mode.set_input_mode(mode));
    }

    /// The system's input mode, from a host whose system decides it (Android's touch mode): it
    /// can't be asked for touch mode any more, as `AndroidComposeView`'s manager.
    pub fn set_system_input_mode(&mut self, mode: crate::input_mode::InputMode) {
        let _turn = compose_runtime::platform::Turn::begin();
        self.with_tree(|tree| tree.input_mode.set_system_input_mode(mode));
    }

    /// Arrows, the D-pad, Enter, Back and Escape move the focus if nothing takes them, as on
    /// Android. Off, Tab alone does (D230).
    pub fn set_keys_navigate(&self, navigate: bool) {
        self.with_tree(|tree| tree.focus.keys_navigate = navigate);
    }

    /// The window's view asks the content for the focus, as `AndroidComposeView.requestFocus`:
    /// whether the content has it now. `false` where the tree is busy.
    pub fn owner_requested_focus(
        &self,
        direction: crate::focus_manager::FocusDirection,
        previous: Option<crate::unit::Rect>,
    ) -> bool {
        let key = self.tree_key;
        crate::focus_requester::on_free_tree(key, |tree| {
            tree.owner_requested_focus(direction, previous)
        })
        .unwrap_or(false)
    }

    /// The window's view lost the focus, as `releaseFocus`.
    pub fn owner_lost_focus(&self) {
        let key = self.tree_key;
        crate::focus_requester::on_free_tree(key, |tree| tree.owner_lost_focus());
    }

    /// What the content asks of the window's view since the last call: to take the focus
    /// (`Some(true)`) or to give it up (`Some(false)`).
    pub fn take_owner_focus_request(&self) -> Option<bool> {
        self.focus_requests_of_host().owner_focus.take()
    }

    /// What asks the system for keyboard mode, for a host whose system decides the input mode.
    pub fn set_keyboard_mode_request(&self, request: impl Fn() -> bool + 'static) {
        self.with_tree(|tree| tree.input_mode.set_keyboard_request(Rc::new(request)));
    }

    /// The input mode manager: told what the system says even while the tree is busy (asking it
    /// for keyboard mode, from a composition, makes the system answer at once).
    pub fn input_mode_manager(&self) -> crate::input_mode::InputModeManager {
        self.with_tree(|tree| tree.input_mode.clone())
    }

    /// Moves focus to `node`'s focus target.
    pub fn request_focus(&mut self, node: NodeId) {
        let _turn = compose_runtime::platform::Turn::begin();
        let actions = self.with_tree(|tree| tree.request_focus(node));
        Self::run(actions);
        self.wake_if_dirty();
    }

    pub fn clear_focus(&mut self) {
        let _turn = compose_runtime::platform::Turn::begin();
        let actions = self.with_tree(|tree| tree.clear_focus());
        Self::run(actions);
        self.wake_if_dirty();
    }

    /// The semantics tree as live handles, as `RootForTest.semanticsOwner`.
    pub fn semantics_owner(&self) -> crate::semantics_node::SemanticsOwner {
        let key = self.with_tree(|tree| tree.focus_requests.tree_key.get());
        crate::semantics_node::SemanticsOwner::new(key)
    }

    pub fn semantics_tree(&self, merged: bool) -> SemanticsSnapshot {
        self.with_tree(|tree| tree.semantics_tree(merged))
    }

    /// The last frame's display list flattened (`DisplayList::flatten`): for tests and dumps.
    /// A renderer draws `UiTree::display_list` (`with_tree`), its groups by reference.
    pub fn display_list(&self) -> FlatDisplayList {
        self.with_tree(|tree| tree.display_list().flatten())
    }

    /// The tree dump: nodes with their bounds, modifiers and semantics.
    pub fn dump_tree(&self) -> String {
        self.with_tree(|tree| tree.dump())
    }
}

/// The host's lifecycle, as an Android view's owner or a desktop window's: resumed from the start
/// unless the host says otherwise (a test's, D837), destroyed with it; and its view models, cleared
/// then unless the platform keeps them (across a configuration change, D838).
struct HostLifecycle {
    registry: LifecycleRegistry,
    view_models: ViewModelStore,
    clears_view_models: bool,
}

impl HostLifecycle {
    fn new() -> HostLifecycle {
        let registry = LifecycleRegistry::new();
        registry.set_current_state(LifecycleState::Resumed);
        HostLifecycle { registry, view_models: ViewModelStore::new(), clears_view_models: true }
    }
}

impl Drop for HostLifecycle {
    fn drop(&mut self) {
        if self.registry.current_state() != LifecycleState::Destroyed {
            self.registry.set_current_state(LifecycleState::Destroyed);
        }
        if self.clears_view_models {
            self.view_models.clear();
        }
    }
}

/// The root's locals that never change, provided outermost.
struct HostLocals {
    lifecycle: compose_lifecycle::Lifecycle,
    view_models: Rc<ViewModelStore>,
    scheduler: crate::prefetch::PrefetchScheduler,
    navigation_events: compose_navigation_event::NavigationEventDispatcher,
    back_dispatcher: crate::back::OnBackPressedDispatcher,
    saveable_registry: compose_runtime::SaveableStateRegistry,
    props: crate::configuration::HostProps,
    events: crate::configuration::HostEvents,
    insets: crate::configuration::WindowInsetsSource,
    window_info: crate::focus_manager::WindowInfo,
}

impl HostLocals {
    /// One provider for them all: each group more is what every local read above the content's
    /// own providers walks (D842).
    fn provide(&self, content: impl FnOnce()) {
        let view_models: compose_lifecycle::ViewModelStoreOwnerRef = self.view_models.clone();
        compose_runtime::CompositionLocalProvider(
            (
                compose_lifecycle::LOCAL_VIEW_MODEL_STORE_OWNER.provides(Some(view_models)),
                compose_lifecycle::LOCAL_LIFECYCLE_OWNER.provides(self.lifecycle.clone()),
                crate::prefetch::LOCAL_PREFETCH_SCHEDULER.provides(Some(self.scheduler.clone())),
                (
                    compose_navigation_event::LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER
                        .provides(Some(self.navigation_events.clone())),
                    crate::back::LOCAL_ON_BACK_PRESSED_DISPATCHER
                        .provides(Some(self.back_dispatcher.clone())),
                    crate::focus_manager::LOCAL_WINDOW_INFO.provides(self.window_info.clone()),
                ),
                compose_runtime::LOCAL_SAVEABLE_STATE_REGISTRY
                    .provides(Some(self.saveable_registry.clone())),
                crate::configuration::LOCAL_HOST_PROPS.provides(self.props.clone()),
                crate::configuration::LOCAL_HOST_EVENTS.provides(self.events.clone()),
                crate::configuration::LOCAL_WINDOW_INSETS_SOURCE.provides(self.insets.clone()),
            ),
            content,
        )
    }
}
