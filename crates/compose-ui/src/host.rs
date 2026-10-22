//! `UiHost`: a composition of a UI tree, with its frame loop and event entry points. Platform
//! hosts (desktop, Android) wrap it; tests drive it headless (docs/07-platforms.md, "The frame
//! loop").

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::{Composition, executor};

use crate::display_list::DisplayList;
use crate::input::{Action, KeyEvent, PointerEvent};
use crate::semantics::SemanticsNode;
use crate::text::{FixedTextMeasurer, TextMeasurer};
use crate::tree::{NodeId, UiTree};
use crate::unit::{Density, IntSize};

/// What a frame did.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FrameStats {
    pub recomposed: bool,
    pub measured: bool,
    pub drew: bool,
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
    /// The layout direction content sees as `LOCAL_LAYOUT_DIRECTION`.
    layout_direction: compose_runtime::OwnedState<crate::alignment::LayoutDirection>,
    /// The window's size, the root's constraints hint (`LOCAL_CONSTRAINTS_HINT`).
    window_size: compose_runtime::OwnedState<IntSize>,
    /// Work lazy lists ask for between frames (`prefetch`), given to content as
    /// `LOCAL_PREFETCH_SCHEDULER`.
    prefetch_scheduler: crate::prefetch::PrefetchScheduler,
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
        // Writes to state read while measuring or drawing ask for a frame, as composition's do.
        let observer = compose_runtime::ReadObserver::new(move || request());
        composition.with_applier(|tree: &mut UiTree| tree.set_read_observer(observer));
        UiHost {
            composition,
            size,
            frame_requested,
            framing,
            pending_content: RefCell::new(None),
            waker,
            density: compose_runtime::mutable_state_of(density),
            layout_direction: compose_runtime::mutable_state_of(
                crate::alignment::LayoutDirection::Ltr,
            ),
            window_size: compose_runtime::mutable_state_of(size),
            prefetch_scheduler: crate::prefetch::PrefetchScheduler::default(),
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

    /// Moves focus as `FocusRequester`s asked, now that the tree is free.
    fn handle_focus_requests(&self) {
        loop {
            let pending = self.with_tree(|tree| tree.focus_requests.pending.take());
            if pending.is_empty() {
                return;
            }
            for node in pending {
                let actions = self.with_tree(|tree| {
                    if tree.get(node).is_some() { tree.request_focus(node) } else { Vec::new() }
                });
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

    pub fn set_density(&mut self, density: Density) {
        self.with_tree(|tree| tree.set_density(density));
        self.density.set(density);
        self.wake();
    }

    /// Runs `f` with the tree.
    pub fn with_tree<R>(&self, f: impl FnOnce(&mut UiTree) -> R) -> R {
        self.composition.with_applier(f)
    }

    /// Whether a frame would do anything.
    pub fn needs_frame(&self) -> bool {
        self.has_pending_work() || executor::has_frame_awaiters()
    }

    /// Whether a frame would do anything besides resuming the tasks waiting for one: state
    /// changed, something to draw, a task ready.
    pub fn has_pending_work(&self) -> bool {
        self.frame_requested.get()
            || self.pending_content.borrow().is_some()
            || self.composition.has_invalidations()
            || self.with_tree(|tree| tree.needs_draw)
            || executor::has_ready_tasks()
            || self.with_tree(|tree| !tree.focus_requests.pending.borrow().is_empty())
    }

    /// One frame: runs the tasks that are due and resumes those waiting for a frame (with this
    /// frame's time), recomposes invalid scopes, measures and draws what changed, then starts the
    /// tasks the pass launched.
    pub fn frame(&mut self) -> FrameStats {
        // The tasks due and those waiting for this frame write state this frame recomposes,
        // measures and draws: they ask for no other frame.
        self.framing.set(true);
        let tasks = crate::trace::section(c"compose-rs: tasks");
        executor::fire_timers();
        executor::run_ready();
        self.handle_focus_requests();
        executor::dispatch_frame();
        drop(tasks);
        let compose = crate::trace::section(c"compose-rs: recompose");
        self.frame_requested.set(false);
        let bodies = compose_runtime::bodies_run();
        let content = self.pending_content.borrow_mut().take();
        let composed = content.is_some();
        if let Some(content) = content {
            let density = self.density.handle();
            let direction = self.layout_direction.handle();
            let window = self.window_size.handle();
            let scheduler = self.prefetch_scheduler.clone();
            let content: compose_runtime::Callback = compose_runtime::callback(content);
            self.composition.set_content(move || {
                let content = content.clone();
                // The prefetch scheduler never changes: outermost, it's read without
                // recomposing anything.
                let scheduler =
                    crate::prefetch::LOCAL_PREFETCH_SCHEDULER.provides(Some(scheduler.clone()));
                compose_runtime::CompositionLocalProvider(scheduler, move || {
                    let content = content.clone();
                    compose_runtime::CompositionLocalProvider(
                        LOCAL_DENSITY.provides(density.get()),
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
                                        move || Content(content),
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
        drop(compose);
        let size = self.size;
        let invalid_before_layout = self.composition.has_invalidations();
        let layout = crate::trace::section(c"compose-rs: layout");
        let mut measured = self.lay_out(size);
        drop(layout);
        // A layout that changed what to compose (a lazy list reaching items it lacks) is
        // composed and laid out again in this frame, a few times at most, before drawing.
        let mut rounds = 0;
        while !invalid_before_layout && rounds < 3 && self.composition.has_invalidations() {
            rounds += 1;
            let _round = crate::trace::section(c"compose-rs: recompose and layout again");
            recomposed |= self.composition.recompose();
            measured |= self.lay_out(size);
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
        if measured {
            let actions = self.with_tree(|tree| tree.refresh_hover());
            Self::run(actions);
        }
        let draw = crate::trace::section(c"compose-rs: draw");
        let (drew, actions) = self.with_tree(|tree| (tree.draw(), tree.take_pending_actions()));
        drop(draw);
        // What composition queued (a focus lost, listeners attached), now that the tree is free.
        Self::run(actions);
        // `LaunchedEffect`s started by the pass take their first step.
        executor::run_ready();
        // What the pass and its effects wrote needs the next frame.
        self.framing.set(false);
        if self.frame_requested.get() {
            self.wake();
        }
        FrameStats {
            recomposed,
            measured,
            drew,
            bodies_run: compose_runtime::bodies_run() - bodies,
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
        if !self.has_prefetch_work() {
            return false;
        }
        if self.has_pending_work() {
            return true;
        }
        let _prefetch = crate::trace::section(c"compose-rs: prefetch");
        // The requests' writes are composed here: they wake nothing.
        self.framing.set(true);
        let more = self.prefetch_scheduler.run(&self.composition, deadline);
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
        executor::run_ready();
        self.wake_if_dirty();
        more
    }

    fn run(actions: Vec<Action>) {
        for action in actions {
            action.run();
        }
    }

    /// Measures and places what the writes observed since invalidated, then runs the layout
    /// callbacks (`on_size_changed`, `on_globally_positioned`). A measurement reading state that
    /// another wrote while laying out (a lazy list's viewport size, which its items'
    /// `fill_parent_max_*` read) is measured again within this layout, twice at most, rather
    /// than at the next frame.
    fn lay_out(&mut self, size: IntSize) -> bool {
        let mut measured = false;
        for _ in 0..3 {
            let again = self.with_tree(|tree| {
                tree.apply_observed_writes();
                tree.layout(size)
            });
            measured |= again;
            if !again {
                break;
            }
        }
        let actions = self.with_tree(|tree| tree.take_layout_callbacks());
        Self::run(actions);
        measured
    }

    /// Dispatches a pointer event (its position in the root), then runs the handlers' actions.
    pub fn send_pointer(&mut self, mut event: PointerEvent) {
        if event.time_nanos == 0 {
            event.time_nanos = executor::now_nanos();
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

    /// Dispatches a key event to the focused node; returns whether it was consumed.
    pub fn send_key(&mut self, event: KeyEvent) -> bool {
        let (consumed, actions) = self.with_tree(|tree| tree.dispatch_key(event));
        Self::run(actions);
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

    /// Moves focus to `node`'s focus target.
    /// Performs a semantics action on `node` (an accessibility service's click or focus), then
    /// runs the handlers' actions. Returns whether the node did it.
    pub fn perform_semantics_action(
        &mut self,
        node: NodeId,
        action: crate::semantics::SemanticsAction,
    ) -> bool {
        let (performed, actions) =
            self.with_tree(|tree| tree.perform_semantics_action(node, action));
        Self::run(actions);
        performed
    }

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

    pub fn display_list(&self) -> DisplayList {
        self.with_tree(|tree| tree.display_list().clone())
    }

    /// The tree dump: nodes with their bounds, modifiers and semantics.
    pub fn dump_tree(&self) -> String {
        self.with_tree(|tree| tree.dump())
    }
}
