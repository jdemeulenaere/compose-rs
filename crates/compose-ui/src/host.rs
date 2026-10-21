//! `UiHost`: a composition of a UI tree, with its frame loop and event entry points. Platform
//! hosts (desktop, Android) wrap it; tests drive it headless (docs/07-platforms.md, "The frame
//! loop").

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::Composition;

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
    waker: Waker,
}

impl UiHost {
    /// A host of `size` pixels at `density`, measuring text with `text_measurer`.
    pub fn new(size: IntSize, density: Density, text_measurer: Box<dyn TextMeasurer>) -> UiHost {
        let composition = Composition::new(UiTree::new(density, text_measurer));
        let frame_requested = Rc::new(Cell::new(true));
        let waker: Waker = Rc::new(RefCell::new(None));
        let (requested, wake) = (frame_requested.clone(), waker.clone());
        composition.set_frame_requester(move || {
            requested.set(true);
            let waker = wake.borrow().clone();
            if let Some(waker) = waker {
                waker();
            }
        });
        UiHost { composition, size, frame_requested, waker }
    }

    /// A headless host at density 1 with the fixed-advance text measurer: for tests.
    pub fn headless(width: i32, height: i32) -> UiHost {
        UiHost::new(
            IntSize::new(width, height),
            Density::default(),
            Box::new(FixedTextMeasurer::new()),
        )
    }

    pub fn set_content(&self, content: impl Fn() + 'static) {
        self.composition.set_content(content);
        self.frame_requested.set(true);
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
            self.frame_requested.set(true);
        }
    }

    pub fn set_density(&mut self, density: Density) {
        self.with_tree(|tree| tree.set_density(density));
        self.frame_requested.set(true);
    }

    /// Runs `f` with the tree.
    pub fn with_tree<R>(&self, f: impl FnOnce(&mut UiTree) -> R) -> R {
        self.composition.with_applier(f)
    }

    /// Whether a frame would do anything.
    pub fn needs_frame(&self) -> bool {
        self.frame_requested.get()
            || self.composition.has_invalidations()
            || self.with_tree(|tree| tree.needs_draw)
    }

    /// One frame: recomposes invalid scopes, then measures and draws what changed.
    pub fn frame(&mut self) -> FrameStats {
        self.frame_requested.set(false);
        let bodies = compose_runtime::bodies_run();
        let recomposed = self.composition.has_invalidations() && self.composition.recompose();
        let size = self.size;
        let (measured, drew) = self.with_tree(|tree| (tree.layout(size), tree.draw()));
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
    }

    /// Dispatches a key event to the focused node; returns whether it was consumed.
    pub fn send_key(&mut self, event: KeyEvent) -> bool {
        let (consumed, actions) = self.with_tree(|tree| tree.dispatch_key(event));
        Self::run(actions);
        consumed
    }

    /// Dispatches committed text to the focused node; returns whether it was taken.
    pub fn send_text(&mut self, text: &str) -> bool {
        let (consumed, actions) = self.with_tree(|tree| tree.dispatch_text(text));
        Self::run(actions);
        consumed
    }

    /// Moves focus to `node`'s focus target.
    pub fn request_focus(&mut self, node: NodeId) {
        let actions = self.with_tree(|tree| tree.request_focus(node));
        Self::run(actions);
    }

    pub fn clear_focus(&mut self) {
        let actions = self.with_tree(|tree| tree.clear_focus());
        Self::run(actions);
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
