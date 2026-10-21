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
}

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
        if self.with_tree(|tree| tree.needs_draw) {
            self.wake();
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
            self.wake();
        }
    }

    pub fn set_density(&mut self, density: Density) {
        self.with_tree(|tree| tree.set_density(density));
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
    }

    /// One frame: runs the tasks that are due and resumes those waiting for a frame (with this
    /// frame's time), recomposes invalid scopes, measures and draws what changed, then starts the
    /// tasks the pass launched.
    pub fn frame(&mut self) -> FrameStats {
        // The tasks due and those waiting for this frame write state this frame recomposes,
        // measures and draws: they ask for no other frame.
        self.framing.set(true);
        executor::fire_timers();
        executor::run_ready();
        executor::dispatch_frame();
        self.frame_requested.set(false);
        let bodies = compose_runtime::bodies_run();
        let content = self.pending_content.borrow_mut().take();
        let composed = content.is_some();
        if let Some(content) = content {
            self.composition.set_content(content);
        }
        let recomposed =
            (self.composition.has_invalidations() && self.composition.recompose()) || composed;
        let size = self.size;
        let invalid_before_layout = self.composition.has_invalidations();
        let mut measured = self.with_tree(|tree| {
            tree.apply_observed_writes();
            tree.layout(size)
        });
        // A layout that changed what to compose (a lazy list reaching items it lacks) is
        // composed and laid out again in this frame, a few times at most, before drawing.
        let mut rounds = 0;
        while !invalid_before_layout && rounds < 3 && self.composition.has_invalidations() {
            rounds += 1;
            self.composition.recompose();
            measured |= self.with_tree(|tree| {
                tree.apply_observed_writes();
                tree.layout(size)
            });
        }
        let (drew, actions) = self.with_tree(|tree| (tree.draw(), tree.take_pending_actions()));
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

    fn run(actions: Vec<Action>) {
        for action in actions {
            action.run();
        }
    }

    /// Dispatches a pointer event (its position in the root), then runs the handlers' actions.
    pub fn send_pointer(&mut self, event: PointerEvent) {
        let actions = self.with_tree(|tree| tree.dispatch_pointer(event));
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
