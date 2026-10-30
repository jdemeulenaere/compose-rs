//! Keeping what a node's content last drew, to draw it after the content has gone: the part of
//! Compose's `GraphicsLayer` that `CapturedAnimatedVisibility` needs (`placeWithLayer` and
//! `drawLayer`), not the object with its properties (D1413).
//!
//! The captured node is a repaint boundary: what the frame lists for it and the groups nested in it
//! is shared, not copied, each time they change. Drawing flattens them, as a renderer would.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use compose_runtime::ComposeArg;

use crate::display_list::{DrawGroup, DrawOp, FlatDisplayList, flatten_groups};
use crate::draw::DrawScope;
use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement};
use crate::text::{ParagraphHandle, TextMeasurer};

#[derive(Default)]
struct Captured {
    groups: Vec<DrawGroup>,
    /// `groups` flattened, once they're asked for.
    flat: Option<FlatDisplayList>,
}

/// What a node's content last drew: a handle, its clones sharing it, kept as long as what
/// remembers it. Written by a node with [`Modifier::capture_content`], drawn by
/// [`DrawScope::draw_captured`].
#[derive(Clone, Default)]
pub struct ContentCapture(Rc<RefCell<Captured>>);

impl ContentCapture {
    pub fn new() -> ContentCapture {
        ContentCapture::default()
    }

    fn store(&self, groups: Vec<DrawGroup>) {
        *self.0.borrow_mut() = Captured { groups, flat: None };
    }

    /// Whether nothing has been captured.
    pub fn is_empty(&self) -> bool {
        self.0.borrow().groups.is_empty()
    }
}

impl ComposeArg for ContentCapture {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl DrawScope<'_> {
    /// Draws what `capture` kept, at this scope's origin, as `drawLayer`: nothing if it kept
    /// nothing.
    pub fn draw_captured(&mut self, capture: &ContentCapture) {
        let mut captured = capture.0.borrow_mut();
        let Captured { groups, flat } = &mut *captured;
        if groups.is_empty() {
            return;
        }
        let flat = flat.get_or_insert_with(|| flatten_groups(groups));
        if flat.ops.is_empty() {
            return;
        }
        let origin = self.origin;
        // Its paints name shaders by index: the list's own from `base` on.
        let base = self.list.shaders.len() as u32;
        self.list.shaders.extend_from_slice(&flat.shaders);
        self.list.push(DrawOp::Translate(origin));
        for op in &flat.ops {
            let mut op = op.clone();
            if let Some(index) = op.paint_mut().and_then(|paint| paint.shader.as_mut()) {
                index.0 += base;
            }
            self.list.push(op);
        }
        self.list.push(DrawOp::Translate(crate::unit::Offset::new(0.0 - origin.x, 0.0 - origin.y)));
    }
}

struct CaptureElement(ContentCapture);

pub(crate) struct CaptureNode(ContentCapture);

impl ModifierNodeElement for CaptureElement {
    type Node = CaptureNode;
    fn create(&self) -> CaptureNode {
        CaptureNode(self.0.clone())
    }
    fn update(&self, node: &mut CaptureNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        self.0.same(&other.0)
    }
    fn name(&self) -> &'static str {
        "capture_content"
    }
}

impl ModifierNode for CaptureNode {
    fn is_repaint_boundary(&self) -> bool {
        true
    }
    fn content_capture(&self) -> Option<&ContentCapture> {
        Some(&self.0)
    }
}

impl Modifier {
    /// Keeps in `capture` what this node and its children last drew, as a `placeWithLayer` of the
    /// content, so it can be drawn once they're gone.
    pub fn capture_content(self, capture: ContentCapture) -> Modifier {
        self.then_custom(CaptureElement(capture))
    }
}

/// The paragraphs captures hold, which a text leaving the tree doesn't take with it: a recording
/// names them by handle, so they stay until nothing that kept them does.
#[derive(Default)]
pub(crate) struct HeldParagraphs {
    captures: Vec<(Weak<RefCell<Captured>>, Vec<ParagraphHandle>)>,
    counts: HashMap<u64, u32>,
    /// Those released while held.
    released: HashSet<u64>,
}

impl HeldParagraphs {
    /// Whether a text leaving the tree can release `paragraph`: if not, it's released when the last
    /// capture holding it lets it go.
    pub(crate) fn release_later(&mut self, paragraph: ParagraphHandle) -> bool {
        let held = self.counts.contains_key(&paragraph.0);
        if held {
            self.released.insert(paragraph.0);
        }
        held
    }

    /// Those of the captures that went are let go.
    pub(crate) fn sweep(&mut self, measurer: &mut dyn TextMeasurer) {
        if self.captures.is_empty() {
            return;
        }
        let mut index = 0;
        while index < self.captures.len() {
            if self.captures[index].0.strong_count() == 0 {
                let (_, paragraphs) = self.captures.swap_remove(index);
                self.let_go(&paragraphs, measurer);
            } else {
                index += 1;
            }
        }
    }

    /// `capture` now holds what `groups` draw, instead of what it held.
    pub(crate) fn store(
        &mut self,
        capture: &ContentCapture,
        groups: Vec<DrawGroup>,
        measurer: &mut dyn TextMeasurer,
    ) {
        let mut paragraphs = Vec::new();
        for group in &groups {
            for op in &group.content.ops {
                if let DrawOp::DrawParagraph(paragraph, _) = op {
                    paragraphs.push(*paragraph);
                }
            }
        }
        // Counted before the old are let go: a paragraph in both stays.
        for paragraph in &paragraphs {
            *self.counts.entry(paragraph.0).or_insert(0) += 1;
        }
        let weak = Rc::downgrade(&capture.0);
        let at = self.captures.iter().position(|(kept, _)| kept.ptr_eq(&weak));
        match at {
            Some(at) => {
                let old = std::mem::replace(&mut self.captures[at].1, paragraphs);
                self.let_go(&old, measurer);
            }
            None => self.captures.push((weak, paragraphs)),
        }
        capture.store(groups);
    }

    fn let_go(&mut self, paragraphs: &[ParagraphHandle], measurer: &mut dyn TextMeasurer) {
        for paragraph in paragraphs {
            let Some(count) = self.counts.get_mut(&paragraph.0) else { continue };
            *count -= 1;
            if *count == 0 {
                self.counts.remove(&paragraph.0);
                if self.released.remove(&paragraph.0) {
                    measurer.release(*paragraph);
                }
            }
        }
    }
}
