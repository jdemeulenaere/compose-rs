//! `UiHost`: a composition of a UI tree, with its frame loop and event entry points. Platform
//! hosts (desktop, Android) wrap it; tests drive it headless (docs/07-platforms.md, "The frame
//! loop").

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::Composition;

use crate::display_list::FlatDisplayList;
use crate::fixed_text::FixedTextMeasurer;
use crate::input::{Action, KeyEvent, PointerEvent};
use crate::semantics::SemanticsNode;
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
    /// Where `BackHandler`s register (`LOCAL_ON_BACK_PRESSED_DISPATCHER`).
    back_dispatcher: crate::back::OnBackPressedDispatcher,
    /// Where `remember_saveable` registers (`LOCAL_SAVEABLE_STATE_REGISTRY`).
    saveable_registry: compose_runtime::SaveableStateRegistry,
    /// The platform's configuration (`LOCAL_CONFIGURATION`).
    configuration: compose_runtime::OwnedState<crate::configuration::Configuration>,
    /// The window's insets (`LOCAL_PLATFORM_WINDOW_INSETS`, and read while measuring).
    window_insets: compose_runtime::OwnedState<crate::configuration::PlatformWindowInsets>,
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
    compose_runtime::CompositionLocal::new("LocalDensity", Density::default);

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
        let window_insets = compose_runtime::mutable_state_of(
            crate::configuration::PlatformWindowInsets::default(),
        );
        let insets = window_insets.handle();
        composition.with_applier(|tree: &mut UiTree| tree.set_window_insets_state(insets));
        UiHost {
            composition,
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
            back_dispatcher: crate::back::OnBackPressedDispatcher::new(),
            saveable_registry: compose_runtime::SaveableStateRegistry::new(None),
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
        if self.with_tree(|tree| tree.needs_draw) {
            self.wake();
        }
    }

    /// Runs the side effects modifier nodes asked for (`NodeContext::side_effect`), now that the
    /// tree is free, as Compose's `onEndApplyChanges`: what they queue in turn runs too.
    fn run_side_effects(&self) {
        loop {
            let effects = self.with_tree(|tree| std::mem::take(&mut tree.side_effects));
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
        loop {
            let pending = self.with_tree(|tree| tree.focus_requests.pending.take());
            if pending.is_empty() {
                return;
            }
            for request in pending {
                let actions = self.with_tree(|tree| tree.perform_focus_request(request));
                Self::run(actions);
            }
        }
    }

    /// Called whenever the host needs a frame (state changed): a platform host schedules one.
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
        if self.size != size {
            self.size = size;
            self.window_size.set(size);
            self.wake();
        }
    }

    /// Lays the content out in `direction` (the window's: right-to-left for Arabic or Hebrew
    /// locales), as `LocalLayoutDirection` at the root.
    pub fn set_layout_direction(&mut self, direction: crate::alignment::LayoutDirection) {
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
        self.with_tree(|tree| tree.set_view_configuration(configuration));
        self.view_configuration.set(configuration);
        self.wake();
    }

    /// The view configuration the detectors use now.
    pub fn view_configuration(&self) -> crate::view_configuration::ViewConfiguration {
        self.with_tree(|tree| tree.view_configuration())
    }

    pub fn set_density(&mut self, density: Density) {
        self.with_tree(|tree| tree.set_density(density));
        self.density.set(density);
        self.wake();
    }

    /// The platform's configuration, which content reads as `LOCAL_CONFIGURATION` (on desktop,
    /// the window's size in dp and the system's dark mode).
    pub fn set_configuration(&mut self, configuration: crate::configuration::Configuration) {
        self.configuration.set(configuration);
    }

    pub fn configuration(&self) -> crate::configuration::Configuration {
        self.configuration.handle().get()
    }

    /// The window's insets (system bars, the keyboard, a cutout), which the insets paddings and
    /// `LOCAL_PLATFORM_WINDOW_INSETS` read.
    pub fn set_window_insets(&mut self, insets: crate::configuration::PlatformWindowInsets) {
        self.window_insets.set(insets);
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
        let handled = self.back_dispatcher.on_back_pressed();
        self.wake_if_dirty();
        handled
    }

    /// Whether a `BackHandler` is enabled: the platform intercepts back only then. It changes
    /// with composition: platforms ask after each frame.
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
        self.frame_requested.get()
            || self.pending_content.borrow().is_some()
            || self.composition.has_invalidations()
            || self.with_tree(|tree| tree.needs_draw)
            || compose_runtime::platform::has_ready_tasks()
            || self.with_tree(|tree| !tree.focus_requests.pending.borrow().is_empty())
            || self.with_tree(|tree| tree.has_node_requests() || !tree.side_effects.is_empty())
            || self.with_tree(|tree| tree.has_beyond_bounds_moves())
    }

    /// One frame: runs the tasks that are due and resumes those waiting for a frame (with this
    /// frame's time), recomposes invalid scopes, measures and draws what changed, then starts the
    /// tasks the pass launched.
    pub fn frame(&mut self) -> FrameStats {
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
                scheduler: self.prefetch_scheduler.clone(),
                back_dispatcher: self.back_dispatcher.clone(),
                saveable_registry: self.saveable_registry.clone(),
                props: self.props.clone(),
                events: self.events.clone(),
            };
            let configuration = self.configuration.handle();
            let insets = self.window_insets.handle();
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
                                crate::alignment::LOCAL_LAYOUT_DIRECTION.provides(direction.get()),
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
                                                move || {
                                                    compose_runtime::CompositionLocalProvider(
                                                        crate::configuration::LOCAL_PLATFORM_WINDOW_INSETS
                                                            .provides(insets.get()),
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
        let _prefetch = crate::trace::section(c"compose-rs: prefetch");
        // The requests' writes are composed here: they wake nothing.
        self.framing.set(true);
        let more = self.prefetch_scheduler.run(&self.composition, deadline, unbounded_first);
        self.run_side_effects();
        // The items composed aren't placed, and those disposed weren't: unless measuring wrote
        // state for others (observed writes, scopes invalidated), nothing needs a frame. Each
        // item is a repaint boundary, so measuring one marks only its own drawing (D155), drawn
        // when a frame places it; the boundary around the list, if any, is marked as the list
        // is (its children changed), and drawn again with the list's next measurement.
        let quiet = self.with_tree(|tree| {
            tree.needs_draw = false;
            tree.apply_observed_writes();
            !tree.needs_draw
        });
        self.framing.set(false);
        if quiet && !self.composition.has_invalidations() {
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
        let _section = crate::trace::section(c"compose-rs: draw ahead");
        self.with_tree(|tree| tree.draw_ahead())
    }

    fn run(actions: Vec<Action>) {
        for action in actions {
            action.run();
        }
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
            let again = self.with_tree(|tree| {
                tree.apply_node_requests();
                tree.apply_observed_writes();
                tree.layout(size)
            });
            measured |= again;
            let actions = self.with_tree(|tree| tree.take_layout_callbacks());
            let called = !actions.is_empty();
            Self::run(actions);
            if !again && !called {
                break;
            }
        }
        measured
    }

    /// Dispatches a pointer event (its position in the root), then runs the handlers' actions.
    pub fn send_pointer(&mut self, mut event: PointerEvent) {
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
        let actions = self.with_tree(|tree| tree.pointer_left());
        Self::run(actions);
        self.wake_if_dirty();
    }

    /// Dispatches a key event to the focused node; returns whether it was consumed. Escape
    /// that nothing focused takes goes back (`on_back_pressed`), as a dialog window takes it
    /// on desktop.
    pub fn send_key(&mut self, event: KeyEvent) -> bool {
        let (consumed, actions) = self.with_tree(|tree| tree.dispatch_key(event));
        Self::run(actions);
        if !consumed
            && event.key == crate::input::Key::Escape
            && event.kind == crate::input::KeyEventType::KeyDown
        {
            return self.on_back_pressed();
        }
        self.wake_if_dirty();
        consumed
    }

    /// Dispatches committed text to the focused node; returns whether it was taken.
    pub fn send_text(&mut self, text: &str) -> bool {
        let (consumed, actions) = self.with_tree(|tree| tree.dispatch_text(text));
        Self::run(actions);
        self.wake_if_dirty();
        consumed
    }

    /// Dispatches an input method's composition to the focused node; returns whether it was
    /// taken.
    pub fn send_preedit(&mut self, text: &str, cursor: Option<(usize, usize)>) -> bool {
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
        let Some(action) = self.semantics_action(node, key) else { return false };
        let done = perform(&action);
        self.handle_focus_requests();
        self.wake_if_dirty();
        done
    }

    /// Moves focus to `node`'s focus target.
    pub fn request_focus(&mut self, node: NodeId) {
        let actions = self.with_tree(|tree| tree.request_focus(node));
        Self::run(actions);
        self.wake_if_dirty();
    }

    pub fn clear_focus(&mut self) {
        let actions = self.with_tree(|tree| tree.clear_focus());
        Self::run(actions);
        self.wake_if_dirty();
    }

    pub fn semantics_tree(&self, merged: bool) -> SemanticsNode {
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

/// The root's locals that never change, provided outermost.
struct HostLocals {
    scheduler: crate::prefetch::PrefetchScheduler,
    back_dispatcher: crate::back::OnBackPressedDispatcher,
    saveable_registry: compose_runtime::SaveableStateRegistry,
    props: crate::configuration::HostProps,
    events: crate::configuration::HostEvents,
}

impl HostLocals {
    fn provide(&self, content: impl FnOnce()) {
        use compose_runtime::CompositionLocalProvider as Provide;
        let scheduler =
            crate::prefetch::LOCAL_PREFETCH_SCHEDULER.provides(Some(self.scheduler.clone()));
        Provide(scheduler, || {
            let dispatcher = crate::back::LOCAL_ON_BACK_PRESSED_DISPATCHER
                .provides(Some(self.back_dispatcher.clone()));
            Provide(dispatcher, || {
                let registry = compose_runtime::LOCAL_SAVEABLE_STATE_REGISTRY
                    .provides(Some(self.saveable_registry.clone()));
                Provide(registry, || {
                    Provide(
                        crate::configuration::LOCAL_HOST_PROPS.provides(self.props.clone()),
                        || {
                            Provide(
                                crate::configuration::LOCAL_HOST_EVENTS
                                    .provides(self.events.clone()),
                                content,
                            )
                        },
                    )
                })
            })
        })
    }
}
