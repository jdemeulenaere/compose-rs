//! Overlays: content drawn over everything else and hit first, positioned in the window, as
//! Compose's popups and dialogs (which Compose shows in windows or layers of their own).
//!
//! An overlay is a node of the tree, composed where its `Popup` or `Dialog` is (so composition
//! locals, state and semantics follow), whose modifier makes it one: after layout the tree
//! positions it in the root with its position provider, from its anchor's bounds (the layout
//! around the popup) and the window's size; it is drawn after the rest of the tree and hit
//! before it.

use std::rc::Rc;

use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement};
use crate::unit::{IntOffset, IntSize, Rect};

/// Where an overlay goes, as Compose's `PopupPositionProvider`: from its anchor's bounds and the
/// window's size (both in the root's pixels) and its own size.
pub trait PopupPositionProvider: 'static {
    /// As `calculatePosition(anchorBounds, windowSize, layoutDirection, popupContentSize)`.
    fn position(
        &self,
        anchor: Rect,
        window: IntSize,
        layout_direction: crate::alignment::LayoutDirection,
        popup: IntSize,
    ) -> IntOffset;
}

/// An overlay covering the window from its top start: a dialog's scrim, a popup's
/// outside-click catcher.
pub struct WindowPosition;

impl PopupPositionProvider for WindowPosition {
    fn position(
        &self,
        _anchor: Rect,
        _window: IntSize,
        _layout_direction: crate::alignment::LayoutDirection,
        _popup: IntSize,
    ) -> IntOffset {
        IntOffset::ZERO
    }
}

pub(crate) struct OverlayElement {
    provider: Rc<dyn PopupPositionProvider>,
    focus_on_open: bool,
    pass_through: bool,
    window: Option<crate::focus_manager::WindowToken>,
}

pub(crate) struct OverlayNode {
    pub(crate) provider: Rc<dyn PopupPositionProvider>,
    pub(crate) focus_on_open: bool,
    /// The first focus went into it.
    pub(crate) focused_once: bool,
    /// What it hits, what's beneath gets too (a non-focusable popup's outside).
    pub(crate) pass_through: bool,
    /// What its content's focus manager and window info find its window by.
    pub(crate) window: Option<crate::focus_manager::WindowToken>,
}

impl ModifierNodeElement for OverlayElement {
    type Node = OverlayNode;
    fn create(&self) -> OverlayNode {
        OverlayNode {
            provider: self.provider.clone(),
            focus_on_open: self.focus_on_open,
            focused_once: false,
            pass_through: self.pass_through,
            window: self.window.clone(),
        }
    }
    fn update(&self, node: &mut OverlayNode) {
        node.provider = self.provider.clone();
        if self.focus_on_open && !node.focus_on_open {
            // Taking the focus again: it moves in, as when it opened.
            node.focused_once = false;
        }
        node.focus_on_open = self.focus_on_open;
        node.pass_through = self.pass_through;
        node.window = self.window.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&other.provider, &self.provider)
            && other.focus_on_open == self.focus_on_open
            && other.pass_through == self.pass_through
    }
    fn name(&self) -> &'static str {
        "overlay"
    }
}

impl ModifierNode for OverlayNode {
    /// Reused for another popup: the focus goes into it again when it opens.
    fn on_reset(&mut self, context: &mut crate::node_context::NodeContext<'_>) {
        self.focused_once = false;
        // Laid out again, where it may take the focus (`position_overlays`).
        context.invalidate_measurement();
    }
}

impl Modifier {
    /// Makes the node an overlay, placed in the window by `provider` after layout, drawn over
    /// everything and hit first; with `focus_on_open`, the focus moves into it when it opens
    /// (a dialog). For `Popup`s and `Dialog`s: the node should be measured with the window's
    /// size (`MeasureScope::window_size`).
    pub fn overlay(self, provider: Rc<dyn PopupPositionProvider>, focus_on_open: bool) -> Modifier {
        self.then_custom(OverlayElement {
            provider,
            focus_on_open,
            pass_through: false,
            window: None,
        })
    }

    /// An overlay taking the focus, a window of its own that `token` names.
    pub(crate) fn overlay_window(
        self,
        provider: Rc<dyn PopupPositionProvider>,
        token: crate::focus_manager::WindowToken,
    ) -> Modifier {
        self.then_custom(OverlayElement {
            provider,
            focus_on_open: true,
            pass_through: false,
            window: Some(token),
        })
    }

    /// An overlay whose hits also reach what's beneath it: a non-focusable popup's outside.
    pub(crate) fn overlay_passing_through(
        self,
        provider: Rc<dyn PopupPositionProvider>,
    ) -> Modifier {
        self.then_custom(OverlayElement {
            provider,
            focus_on_open: false,
            pass_through: true,
            window: None,
        })
    }
}
