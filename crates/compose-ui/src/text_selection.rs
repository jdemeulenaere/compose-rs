//! Selecting non-editable text, as Compose's `SelectionContainer`: a press and drag inside a
//! container selects from where it went down to the pointer, across the texts in it (in
//! order); a double-click selects a word, a triple-click a whole text; Cmd+A (Ctrl+A) selects
//! all and Cmd+C (Ctrl+C) copies, the texts joined by new lines. A click elsewhere in it, or
//! the container losing focus, clears the selection. `DisableSelection` leaves its content out.
//!
//! The texts are `Selectable` modifier nodes (foundation's `BasicText`); the tree finds them
//! below the container when a gesture asks, as it does focus targets.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use compose_runtime::OwnedState;
use smallvec::SmallVec;

use crate::input::{
    EventContext, Key, KeyEvent, KeyEventType, PointerEvent, PointerEventPass, PointerEventType,
};
use crate::modifier::{
    FocusTargetModifierNode, KeyInputModifierNode, Modifier, ModifierNode, ModifierNodeElement,
    PointerInputModifierNode,
};
use crate::node_chain::NodeKinds;
use crate::text::{SharedString, SharedTextMeasurer};
use crate::text_attributes::ResolvedTextDirection;
use crate::tree::{NodeId, UiTree};
use crate::unit::{IntSize, Offset, Rect};

/// A text a `SelectionContainer` can select in, as Compose's `Selectable`: a modifier node
/// returning itself from `ModifierNode::as_selectable`. Offsets are byte offsets at character
/// boundaries; positions are in the node's coordinates.
pub trait Selectable {
    /// Its text: what's selected and copied.
    fn text(&self) -> SharedString;
    /// The offset nearest to `position`, from the measurer (`None` before it's laid out).
    fn offset_at(&self, position: Offset, measurer: &SharedTextMeasurer) -> Option<usize>;
    /// The word at `offset`, as `getWordBoundary` (`platform::word_boundary`): what a
    /// double-click selects.
    fn word_at(&self, offset: usize) -> Range<usize>;
    /// Bumped when its text changes: a selection ending in it is dropped then.
    fn version(&self) -> u64 {
        0
    }
    /// Shows `range` selected (highlighted), or nothing. Returns whether that changed.
    fn set_selection(&mut self, range: Option<Range<usize>>, measurer: &SharedTextMeasurer)
    -> bool;
    /// Where a selection handle at `offset` points, in the node's coordinates, as Compose's
    /// `Selectable.getHandlePosition`: the start handle (`start`) or the end's, the selection
    /// reversed or not (`crossed`). `None` before it's laid out.
    fn handle_position(
        &self,
        _offset: usize,
        _start: bool,
        _crossed: bool,
        _measurer: &SharedTextMeasurer,
    ) -> Option<Offset> {
        None
    }
    /// The direction of the text at `offset`, as `getBidiRunDirection`: what a selection's
    /// anchor there says (`Selection.AnchorInfo.direction`).
    fn direction_at(
        &self,
        _offset: usize,
        _measurer: &SharedTextMeasurer,
    ) -> ResolvedTextDirection {
        ResolvedTextDirection::Ltr
    }
    /// Character `offset`'s box, as `getBoundingBox`: the offset kept within the text (past it,
    /// its last character's); empty for an empty text or before it's laid out.
    fn bounding_box(&self, _offset: usize, _measurer: &SharedTextMeasurer) -> Rect {
        Rect::ZERO
    }
    /// The cursor's rect at `offset`, as `getCursorRect`: no width, its line's height; the
    /// offset kept within the text. Empty before it's laid out.
    fn cursor_rect(&self, _offset: usize, _measurer: &SharedTextMeasurer) -> Rect {
        Rect::ZERO
    }
    /// The line around `offset`, as `getRangeOfLineContaining`: from its start to its visible
    /// end, the offset kept to what's shown; empty for an empty text.
    fn range_of_line_containing(
        &self,
        _offset: usize,
        _measurer: &SharedTextMeasurer,
    ) -> Range<usize> {
        0..0
    }
    /// The end of what's shown, as `getLastVisibleOffset`: the last line shown's visible end
    /// (the most lines, or the height, cut the rest).
    fn last_visible_offset(&self, _measurer: &SharedTextMeasurer) -> usize {
        0
    }
    /// The height of the line at `offset`, as `getLineHeight`: none past the lines shown or an
    /// empty text.
    fn line_height(&self, _offset: usize, _measurer: &SharedTextMeasurer) -> f32 {
        0.0
    }
    /// The left of the line at `offset`, as `getLineLeft`: `None` (Compose's `-1f`) past the
    /// lines laid out or before it's laid out. What the magnifier is kept within.
    fn line_left(&self, _offset: usize, _measurer: &SharedTextMeasurer) -> Option<f32> {
        None
    }
    /// The right of the line at `offset`, as `getLineRight`.
    fn line_right(&self, _offset: usize, _measurer: &SharedTextMeasurer) -> Option<f32> {
        None
    }
    /// The middle of the line at `offset`, between its top and bottom, as
    /// `getCenterYForOffset`: where the magnifier is.
    fn center_y_for_offset(&self, _offset: usize, _measurer: &SharedTextMeasurer) -> Option<f32> {
        None
    }
}

/// What a press selects, and a drag after it extends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Unit {
    Character,
    Word,
    /// A paragraph of a text, between line feeds, as Compose's `SelectionAdjustment.Paragraph`.
    Paragraph,
}

/// A selection gesture or command, handled by the tree once the event's dispatch is over.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum SelectionRequest {
    /// A press at `position` (in the root) starts a selection.
    Start {
        container: NodeId,
        position: Offset,
        unit: Unit,
    },
    /// The pointer, dragged to `position`, moves the selection's end there.
    Extend {
        container: NodeId,
        position: Offset,
    },
    SelectAll {
        container: NodeId,
    },
    Copy {
        container: NodeId,
    },
    Clear {
        container: NodeId,
    },
    /// A finger's long press at `position` (in the root) selects the word of the text under it,
    /// as Compose's `SelectionController`'s `longPressDragObserver`; a later press of a multi-tap
    /// selects its word or more (`unit`) at once.
    LongPress {
        container: NodeId,
        position: Offset,
        unit: Unit,
    },
    /// The container's focus or touch mode changed: its handles follow.
    RefreshHandles {
        container: NodeId,
    },
    /// A selection handle of the container is dragged: the finger's movement since the drag
    /// passed the touch slop, or `None` once it ends.
    DragHandle {
        container: NodeId,
        start: bool,
        moved: Option<Offset>,
    },
    /// A gesture selecting ended (a long press's or a mouse's press lifted), as Compose's
    /// `onSelectionUpdateEndCallback`: the toolbar may show.
    GestureEnded {
        container: NodeId,
    },
}

/// Which selection handles a container shows, as the condition Compose's `SelectionContainer`
/// composes them under (touch mode, focus, a selection not trivially collapsed): the direction
/// at each end (`Selection.AnchorInfo.direction`) and whether they're crossed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ContainerHandlesShown {
    pub start_direction: ResolvedTextDirection,
    pub end_direction: ResolvedTextDirection,
    pub crossed: bool,
}

/// What a `SelectionContainer`'s handles share with the tree that holds its selection, as
/// Compose's `SelectionManager`'s `startHandlePosition` and `endHandlePosition`: which show (read
/// by composition), where each points in the container's coordinates (`None`: hidden, as
/// `Offset.Unspecified`), and the drag under way. Foundation's `SelectionContainer` makes one
/// and composes the handles; the tree writes it after each selection change and layout.
#[doc(hidden)]
pub struct ContainerHandles {
    shown: OwnedState<Option<ContainerHandlesShown>>,
    positions: OwnedState<[Option<Offset>; 2]>,
    /// The container's node, once its handles were first refreshed.
    container: Cell<Option<NodeId>>,
    /// The handle dragged (the start's?), where its drag began and where it was when the
    /// selection last followed it, in the container's coordinates, as `handleDragObserver`'s
    /// first and later `dragBeginPosition`.
    drag: Cell<Option<(bool, Offset, Offset)>>,
    /// The point in the texts the selection last followed the drag to (the previous layout's
    /// raw offset, Compose's `givenRawPreviousHandleOffset`), none before the first move.
    drag_point: Cell<Option<Point>>,
    /// What a selection's change vibrates with in touch mode (`LocalHapticFeedback`'s where the
    /// container is composed), as `SelectionManager.hapticFeedBack`.
    haptics: std::cell::RefCell<Option<Rc<dyn crate::platform_services::HapticFeedback>>>,
    /// The tree holding its selection and the container's coordinates, once its node attached.
    tree: Cell<Option<u32>>,
    coordinates: Cell<Option<crate::coordinates::LayoutCoordinates>>,
    /// Whether the toolbar should show, as Compose's `SelectionManager.showToolbar`: set when a
    /// gesture ends (a long press, a mouse's drag, a handle's drag), by select all; cleared when
    /// one starts and when the selection's let go of (D2041).
    show_toolbar: Cell<bool>,
    /// The toolbar was asked to show and not to hide since.
    toolbar_requested: Cell<bool>,
    /// What shows or hides the toolbar (foundation's `ToolbarRequester`), called from a task.
    toolbar_requests: std::cell::RefCell<Option<ToolbarRequests>>,
    /// What the toolbar and the menu read, made when first asked (D2041).
    toolbar: std::cell::OnceCell<ContainerToolbar>,
    /// What its magnifier follows, made where the platform has one (D2073).
    magnifier: std::cell::OnceCell<ContainerMagnifier>,
}

/// What a container's magnifier follows, as Compose's `SelectionManager` state the selection's
/// magnifier reads (D2073).
struct ContainerMagnifier {
    /// The end a gesture moves (the start's?) and where the gesture is, in the container's
    /// coordinates, as Compose's `draggingHandle` and `currentDragPosition`: from a handle's
    /// press or a selection gesture's start to its end.
    drag: Cell<Option<(bool, Offset)>>,
    /// Where it's centered, in the container's coordinates, as Compose's
    /// `calculateSelectionMagnifierCenterAndroid`: unspecified while it's hidden (no gesture in
    /// touch mode over a selection not trivially collapsed, Compose's `shouldShowMagnifier`).
    center: OwnedState<Offset>,
    /// The magnifier's size, once made (zero before): a drag farther than half of it from the
    /// line hides it.
    size: Cell<IntSize>,
}

/// What shows (`true`) or hides a container's toolbar.
type ToolbarRequests = Rc<dyn Fn(bool)>;

/// What a container's toolbar and menu read: states, so they follow them.
struct ContainerToolbar {
    /// Where the toolbar goes, in the root, as Compose's `derivedContentRect`; the last kept
    /// while it's hidden.
    content_rect: OwnedState<Option<Rect>>,
    /// Bumped at each change of the container's selection: what the menu's items follow.
    changes: OwnedState<u32>,
}

/// Which of a container's items show, as Compose's `addSelectionContainerTextContextMenuComponents`:
/// copy over a selection of some characters (`isNonEmptySelection`), select all while not all of
/// its texts are selected (`isEntireContainerSelected`).
#[doc(hidden)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ContainerMenuItems {
    pub copy: bool,
    pub select_all: bool,
}

/// An item of a container's menu, as `addSelectionContainerTextContextMenuComponents`' items.
#[doc(hidden)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContainerMenuAction {
    Copy,
    SelectAll,
}

impl ContainerHandles {
    #[allow(clippy::new_without_default)]
    pub fn new() -> ContainerHandles {
        ContainerHandles {
            shown: compose_runtime::mutable_state_of(None),
            positions: compose_runtime::mutable_state_of([None, None]),
            container: Cell::new(None),
            drag: Cell::new(None),
            drag_point: Cell::new(None),
            haptics: std::cell::RefCell::new(None),
            tree: Cell::new(None),
            coordinates: Cell::new(None),
            show_toolbar: Cell::new(false),
            toolbar_requested: Cell::new(false),
            toolbar_requests: std::cell::RefCell::new(None),
            toolbar: std::cell::OnceCell::new(),
            magnifier: std::cell::OnceCell::new(),
        }
    }

    /// The platform has a magnifier: its selection drags show it (D2073). Asked before the
    /// first drag; a container on a platform without one keeps nothing for it.
    pub fn enable_magnifier(&self) {
        self.magnifier.get_or_init(|| ContainerMagnifier {
            drag: Cell::new(None),
            center: compose_runtime::mutable_state_of(Offset::UNSPECIFIED),
            size: Cell::new(IntSize::ZERO),
        });
    }

    /// Whether it has a magnifier (`enable_magnifier`).
    pub fn has_magnifier(&self) -> bool {
        self.magnifier.get().is_some()
    }

    /// Where its magnifier is centered, in the container's coordinates (a read of its state):
    /// unspecified while hidden.
    pub fn magnifier_center(&self) -> Offset {
        self.magnifier.get().map_or(Offset::UNSPECIFIED, |magnifier| magnifier.center.get())
    }

    /// The magnifier's size, once made: where it hides follows (a refresh from a task).
    pub fn set_magnifier_size(&self, size: IntSize) {
        let Some(magnifier) = self.magnifier.get() else { return };
        if magnifier.size.replace(size) != size {
            self.refresh_later();
        }
    }

    /// The end a gesture moves and where the gesture is (in the container's coordinates), or
    /// none once it ended.
    fn set_magnifier_drag(&self, drag: Option<(bool, Offset)>) {
        if let Some(magnifier) = self.magnifier.get() {
            magnifier.drag.set(drag);
        }
    }

    fn toolbar_states(&self) -> &ContainerToolbar {
        self.toolbar.get_or_init(|| ContainerToolbar {
            content_rect: compose_runtime::mutable_state_of(None),
            changes: compose_runtime::mutable_state_of(0),
        })
    }

    /// What shows (`true`) or hides its toolbar, as Compose's `toolbarRequester`: called from a
    /// task, outside the tree's work, when the tree decides (`updateSelectionToolbar`).
    pub fn set_toolbar_requests(&self, requests: Rc<dyn Fn(bool)>) {
        *self.toolbar_requests.borrow_mut() = Some(requests);
    }

    /// The container's coordinates, once its node attached: what the toolbar's rect, in the
    /// root, is translated from.
    pub fn coordinates(&self) -> Option<crate::coordinates::LayoutCoordinates> {
        self.coordinates.get()
    }

    /// Where the toolbar goes, in the root (a read of its state), as Compose's
    /// `derivedContentRect`: the selected region kept to the container's visible bounds, four
    /// handles' heights lower; `None` before it first showed.
    pub fn toolbar_content_rect(&self) -> Option<Rect> {
        self.toolbar_states().content_rect.get()
    }

    /// Which of its menu's items show, from the tree `context` is in, as Compose's builder reads
    /// the selection when collected; a read of the selection's changes, so a toolbar showing them
    /// follows it.
    pub fn menu_items(
        &self,
        context: &mut crate::node_context::NodeContext<'_>,
    ) -> ContainerMenuItems {
        self.toolbar_states().changes.get();
        let Some(container) = self.container.get() else { return ContainerMenuItems::default() };
        context.tree.enter().container_menu_items(container)
    }

    /// The same, read from its tree where no context is (a menu built in composition, as
    /// Compose's builder reads the selection while its menu shows); `None` while the tree can't
    /// be read. A read of the selection's changes too.
    pub fn current_menu_items(&self) -> Option<ContainerMenuItems> {
        self.toolbar_states().changes.get();
        let Some(container) = self.container.get() else {
            return Some(ContainerMenuItems::default());
        };
        let key = self.tree.get()?;
        let mut items = None;
        crate::node_context::peek_tree(key, &mut |tree| {
            items = Some(tree.container_menu_items(container));
        });
        items
    }

    /// Runs its menu's `action`, as Compose's container's items: copy copies, then lets go of the
    /// selection in touch mode (`onRelease`); select all selects every text, asks for the
    /// focus and the toolbar. Returns whether the menu closes: always for copy, for select all
    /// only while no toolbar is asked for in touch mode (`!showToolbar || !isInTouchMode`).
    /// Asked where the tree can be lent (an item's click); asked while it's busy, it would do
    /// nothing and close the menu, which a debug build asserts against.
    pub fn run_menu_action(&self, action: ContainerMenuAction) -> bool {
        let (Some(key), Some(container)) = (self.tree.get(), self.container.get()) else {
            return true;
        };
        let mut close = true;
        let ran = crate::node_context::with_free_tree(key, &mut |tree| {
            let Some(found) = tree.container_handles(container) else { return };
            match action {
                ContainerMenuAction::Copy => {
                    tree.handle_selection(SelectionRequest::Copy { container });
                    if found.touch {
                        tree.handle_selection(SelectionRequest::Clear { container });
                    }
                }
                ContainerMenuAction::SelectAll => {
                    tree.handle_selection(SelectionRequest::SelectAll { container });
                    // The focus asked for once something's selected, as Compose's `selectAll`.
                    let selected = tree
                        .text_selection
                        .is_some_and(|selection| selection.container == container);
                    if selected && !found.focused {
                        let direction = crate::focus_manager::FocusDirection::Enter;
                        tree.request_focus_at_or_below(container, direction);
                    }
                    close = !self.show_toolbar.get() || !found.touch;
                }
            }
        });
        debug_assert!(ran, "compose-ui: a container's menu action asked while its tree was busy");
        close
    }

    /// A handle's drag ended or it was tapped: the toolbar shows again, as Compose's
    /// `handleDragObserver`'s `done`.
    fn handle_released(&self) {
        // The gesture's over: no magnifier, as Compose's `done()` clears `draggingHandle`.
        self.set_magnifier_drag(None);
        self.set_show_toolbar_later(true);
    }

    /// Sets whether the toolbar should show; the tree follows from a task (no event's context
    /// here).
    fn set_show_toolbar_later(&self, show: bool) {
        self.show_toolbar.set(show);
        self.refresh_later();
    }

    /// The tree refreshes the handles, toolbar and magnifier from a task.
    fn refresh_later(&self) {
        let (Some(key), Some(container)) = (self.tree.get(), self.container.get()) else { return };
        compose_runtime::platform::launch(async move {
            crate::node_context::with_free_tree(key, &mut |tree| tree.refresh_handles(container));
        });
    }

    /// The start handle (`start`) or the end's was pressed: the toolbar hides while it's held,
    /// as Compose's handle observer's `onDown`; nothing for a hidden handle, as its.
    pub fn handle_pressed(&self, start: bool) {
        let pointed =
            compose_runtime::snapshots::Snapshot::without_read_observation(|| self.position(start));
        if let Some(pointed) = pointed {
            // The magnifier shows from the press, where the drag would begin (a pixel up), as
            // Compose's `onDown` sets `currentDragPosition`.
            self.set_magnifier_drag(Some((start, Offset::new(pointed.x, pointed.y - 1.0))));
            self.set_show_toolbar_later(false);
        }
    }

    /// A handle was tapped (pressed and lifted before the touch slop), as Compose's `onUp`.
    pub fn handle_tapped(&self) {
        self.handle_released();
    }

    /// A handle's press was cancelled before the touch slop: the toolbar shows again, as
    /// Compose's `onUp` (a cancel ends the press with an up there), then `done()`.
    pub fn handle_press_cancelled(&self) {
        self.handle_released();
    }

    /// What its selection's changes vibrate with in touch mode, as the container composed reads
    /// `LocalHapticFeedback`.
    pub fn set_haptic_feedback(&self, haptics: Rc<dyn crate::platform_services::HapticFeedback>) {
        let mut slot = self.haptics.borrow_mut();
        if !slot.as_ref().is_some_and(|old| Rc::ptr_eq(old, &haptics)) {
            *slot = Some(haptics);
        }
    }

    /// Which handles show (a read of its state).
    pub fn shown(&self) -> Option<ContainerHandlesShown> {
        self.shown.get()
    }

    /// Where the start handle (`start`) or the end's points, in the container's coordinates, or
    /// `None` when it's hidden (a read of its state).
    pub fn position(&self, start: bool) -> Option<Offset> {
        self.positions.with(|positions| positions[if start { 0 } else { 1 }])
    }

    /// A handle's drag: the finger's movement since it passed the touch slop, as
    /// `handleDragObserver`'s `onDrag`.
    pub fn drag(&self, context: &mut EventContext<'_>, start: bool, moved: Offset) {
        let Some(container) = self.container.get() else { return };
        let moved = Some(moved);
        context.requests.selection.push(SelectionRequest::DragHandle { container, start, moved });
    }

    /// The handle's drag ended (lifted, cancelled, or its node gone), as `onStop`: its next drag
    /// begins anew where it points, and the toolbar shows again.
    pub fn end_drag(&self) {
        let dragged = self.drag.take().is_some();
        self.drag_point.set(None);
        if dragged {
            self.handle_released();
        } else if self.magnifier.get().is_some_and(|magnifier| magnifier.drag.get().is_some()) {
            self.set_magnifier_drag(None);
            self.refresh_later();
        }
    }

    /// The handle dragged, if any (`SelectionManager.draggingHandle`).
    pub fn dragging(&self) -> Option<bool> {
        self.drag.get().map(|(start, _, _)| start)
    }
}

impl compose_runtime::ComposeArg for ContainerHandles {
    fn same(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// A point in a container's texts: a text (its node, the chain index of its selectable, and
/// the version of its text) and an offset in it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Point {
    node: NodeId,
    index: usize,
    version: u64,
    offset: usize,
}

/// The selection in a container: from `anchor` (where it started) to `focus` (where it ends).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct TextSelection {
    container: NodeId,
    anchor: Point,
    focus: Point,
    unit: Unit,
}

/// Where a selection's container and the texts of its ends were, in the root, with their sizes:
/// what its handles follow, as Compose's `updateHandleOffsets` runs when the container's
/// position or a selected text's coordinates change.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct HandlesPlacedFor([(NodeId, Offset, IntSize); 3]);

/// A text below a container: its node, the chain index of its selectable, its bounds in the
/// root, and its text's version.
#[derive(Clone, Copy, Debug)]
struct Entry {
    node: NodeId,
    index: usize,
    origin: Offset,
    size: IntSize,
    version: u64,
}

impl Entry {
    fn point(&self, offset: usize) -> Point {
        Point { node: self.node, index: self.index, version: self.version, offset }
    }

    fn holds(&self, point: &Point) -> bool {
        self.node == point.node && self.index == point.index && self.version == point.version
    }
}

/// Where a selection's moving end is (`UiTree::point_at`): its point, whether it's on its
/// text, and whether it's in the gap before it (the other end after it: crossed).
#[derive(Clone, Copy)]
struct Placed {
    point: Point,
    on: bool,
    before: bool,
}

/// A selection's ends as places in a container's texts: `(order, offset)`, first to last.
type Span = ((usize, usize), (usize, usize));

impl UiTree {
    /// The selectable texts below `container`, in order: its subtree's nodes in placement
    /// order, without what a nested container or a `DisableSelection` modifier holds.
    fn selectables(&mut self, container: NodeId) -> Vec<Entry> {
        let mut entries = Vec::new();
        let mut stack = vec![container];
        while let Some(node) = stack.pop() {
            let Some(value) = self.get(node) else { continue };
            if !value.placed {
                continue;
            }
            if node != container
                && value.chain.iter().any(|entry| {
                    entry.node.downcast_ref::<SelectionContainerNode>().is_some()
                        || entry.node.downcast_ref::<DisableSelectionNode>().is_some()
                })
            {
                continue;
            }
            let mut layer = 0;
            for index in 0..self.node(node).chain.len() {
                let bits = self.node(node).chain[index].kinds;
                let version = self
                    .get_mut(node)
                    .and_then(|value| value.chain[index].selectable().map(|text| text.version()));
                if let Some(version) = version {
                    let origin = self.layer_origin(node, layer);
                    let size = self.node(node).layers[layer].measured;
                    entries.push(Entry { node, index, origin, size, version });
                }
                if bits.any(NodeKinds::LAYOUT) {
                    layer += 1;
                }
            }
            let order = self.placement_order(node);
            let count = order.as_ref().map_or(self.node(node).children.len(), |order| order.len());
            // Pushed last first, so the first placed comes off the stack first.
            for position in (0..count).rev() {
                let index = order.as_ref().map_or(position, |order| order[position] as usize);
                stack.push(self.node(node).children[index].node);
            }
        }
        entries
    }

    fn selectable_text(&mut self, node: NodeId, index: usize) -> SharedString {
        self.get_mut(node)
            .and_then(|value| value.chain[index].selectable().map(|text| text.text()))
            .unwrap_or(SharedString::EMPTY)
    }

    /// The offset nearest to `position` (in the root) in `entry`'s text.
    fn offset_in(&mut self, entry: &Entry, position: Offset) -> usize {
        let local = position - entry.origin;
        self.node_and_measurer(entry.node)
            .and_then(|(value, (measurer, _))| {
                value.chain[entry.index].selectable()?.offset_at(local, measurer)
            })
            .unwrap_or(0)
    }

    /// The text a finger at `position` (in the root) touches: the one it's on, else the nearest
    /// whose minimum touch target (its bounds grown to `minimum_touch_target_size`, centered) it's
    /// in, as Compose's `hitTestMinimumTouchTarget` picks between overlapping targets (the first
    /// of those as near). Only within the container: its node takes the touches (D2029).
    fn touched_text(
        &mut self,
        container: NodeId,
        entries: &[Entry],
        position: Offset,
    ) -> Option<Entry> {
        let minimum = self.view_configuration_of(container).minimum_touch_target_size;
        let density = self.node(container).density;
        let (width, height) = (density.to_px(minimum.width), density.to_px(minimum.height));
        let mut nearest: Option<(f32, Entry)> = None;
        for entry in entries {
            let size = (entry.size.width as f32, entry.size.height as f32);
            let local = position - entry.origin;
            // How far past each side it is, and how far its target reaches.
            let past = ((-local.x).max(local.x - size.0), (-local.y).max(local.y - size.1));
            let reach = (((width - size.0) / 2.0).max(0.0), ((height - size.1) / 2.0).max(0.0));
            if past.0 > reach.0 || past.1 > reach.1 {
                continue;
            }
            let distance = past.0.max(0.0).powi(2) + past.1.max(0.0).powi(2);
            if nearest.is_none_or(|(best, _)| distance < best) {
                nearest = Some((distance, *entry));
            }
        }
        nearest.map(|(_, entry)| entry)
    }

    /// Where a selection's moving end at `position` (in the root) is, as Compose's
    /// `SelectionLayoutBuilder` places it (D2028). The texts go in order; each is before, on or
    /// after the position (Compose's `resolve2dDirection`: by its rows, beside them by its
    /// columns), and the other end (`anchor`, its text's order; the position itself for a new
    /// selection) before, on or after it by order. A text both ends are past on the same side is
    /// left out (`isSelected`); a mouse out of touch mode selects between texts
    /// (`allowSelectionBetweenSelectables`): then only those both are before once one is kept,
    /// and those both are after but the last. The end is on the first text kept that it's on,
    /// or in the gap before the first kept that it's before (always the first kept: its point
    /// there, the other end after it), or past the last kept (its point in it): Compose's slots
    /// and `startOrEndSlotToIndex`, which round a gap's end to the text on the other end's side.
    /// The offset is `getOffsetForPosition`'s, kept to the text's start above it and to its end
    /// below.
    fn point_at(
        &mut self,
        container: NodeId,
        entries: &[Entry],
        position: Offset,
        anchor: Option<usize>,
    ) -> Option<Placed> {
        let between = !self.container_in_touch_mode(container);
        let side = |at: f32, from: f32, to: f32| {
            if at < from {
                -1
            } else if at > to {
                1
            } else {
                0
            }
        };
        let mut found = None;
        let mut last_kept = None;
        for (order, entry) in entries.iter().enumerate() {
            let (origin, size) = (entry.origin, entry.size);
            let current = match side(position.y, origin.y, origin.y + size.height as f32) {
                0 => side(position.x, origin.x, origin.x + size.width as f32),
                y => y,
            };
            let other = match anchor {
                None => current,
                // Before this text when it's earlier in order (`getDirectionById`).
                Some(anchor) => (anchor as isize - order as isize).signum() as i32,
            };
            let left_out = if between {
                (current == 1 && other == 1 && order + 1 != entries.len())
                    || (current == -1 && other == -1 && last_kept.is_some())
            } else {
                current != 0 && current == other
            };
            if left_out {
                continue;
            }
            if current != 1 {
                found = Some((order, current == 0));
                break;
            }
            last_kept = Some(order);
        }
        let (index, on, before) = match found {
            Some((index, on)) => (index, on, !on),
            None => (last_kept?, false, false),
        };
        let entry = entries[index];
        let local = position - entry.origin;
        let offset = if local.y <= 0.0 {
            0
        } else if local.y >= entry.size.height as f32 {
            self.selectable_text(entry.node, entry.index).len()
        } else {
            self.offset_in(&entry, position)
        };
        Some(Placed { point: entry.point(offset), on, before })
    }

    /// The selection's ends in `entries`' order, or `None` if a text it ends in is gone or
    /// changed (the selection is then dropped, as Compose's).
    fn span(selection: &TextSelection, entries: &[Entry]) -> Option<Span> {
        let place = |point: &Point| {
            entries.iter().position(|entry| entry.holds(point)).map(|order| (order, point.offset))
        };
        let (anchor, focus) = (place(&selection.anchor)?, place(&selection.focus)?);
        Some(if anchor <= focus { (anchor, focus) } else { (focus, anchor) })
    }

    /// The word around `point`.
    fn word(&mut self, point: Point) -> Range<usize> {
        self.get_mut(point.node)
            .and_then(|value| {
                value.chain[point.index].selectable().map(|text| text.word_at(point.offset))
            })
            .unwrap_or(point.offset..point.offset)
    }

    /// The paragraph around `point` in its text, between the line feeds before and after it, as
    /// Compose's `getParagraphBoundary`.
    fn paragraph(&mut self, point: Point) -> Range<usize> {
        let text = self.selectable_text(point.node, point.index);
        let at = point.offset.min(text.len());
        let start =
            text.as_bytes()[..at].iter().rposition(|&byte| byte == b'\n').map_or(0, |it| it + 1);
        let end = text.as_bytes()[at..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(text.len(), |it| at + it);
        start..end
    }

    fn order_of(entries: &[Entry], point: &Point) -> Option<(usize, usize)> {
        entries.iter().position(|entry| entry.holds(point)).map(|order| (order, point.offset))
    }

    /// Handles a selection gesture or command; the handles of the containers it touched follow.
    pub(crate) fn handle_selection(&mut self, request: SelectionRequest) {
        let before = self.text_selection;
        let container = request.container();
        self.apply_selection_request(request);
        self.after_selection_change(container, before);
        if let Some(old) =
            before.map(|selection| selection.container).filter(|old| *old != container)
        {
            self.after_selection_change(old, before);
        }
    }

    /// `container`'s handles follow its selection, and a change of it vibrates in touch mode, as
    /// Compose's `TextHandleMove` feedback when a gesture changes the selection or `onRelease`
    /// clears it.
    fn after_selection_change(&mut self, container: NodeId, before: Option<TextSelection>) {
        let of = |selection: Option<TextSelection>| {
            selection
                .filter(|selection| selection.container == container)
                .map(|selection| (selection.anchor, selection.focus))
        };
        if of(before) != of(self.text_selection)
            && let Some(found) = self.container_handles(container)
        {
            // The menu's items follow the selection, once a menu asked for them.
            if let Some(toolbar) = found.handles.toolbar.get() {
                let changes =
                    compose_runtime::snapshots::Snapshot::without_read_observation(|| {
                        toolbar.changes.get()
                    });
                toolbar.changes.set(changes.wrapping_add(1));
            }
            if found.touch
                && let Some(haptics) = found.handles.haptics.borrow().clone()
            {
                haptics.perform_haptic_feedback(
                    crate::platform_services::HapticFeedbackType::TextHandleMove,
                );
            }
        }
        self.refresh_handles(container);
    }

    /// After a layout: a selection whose end's text changed or left is let go of, as Compose's
    /// `onSelectableChangeCallback` (`onRelease`, then `selection = null`), so its handles,
    /// toolbar and menu follow at once; it was dropped only when next asked for before.
    pub(crate) fn release_selection_if_its_texts_changed(&mut self) {
        let Some(selection) = self.text_selection else { return };
        let holds = |tree: &mut UiTree, point: &Point| {
            let version = tree.get_mut(point.node).and_then(|value| {
                value.chain.get_mut(point.index)?.selectable().map(|text| text.version())
            });
            version == Some(point.version)
        };
        if holds(self, &selection.anchor) && holds(self, &selection.focus) {
            return;
        }
        self.handle_selection(SelectionRequest::Clear { container: selection.container });
    }

    /// Sets whether `container`'s toolbar should show, as Compose's `showSelectionToolbar` and
    /// `hideSelectionToolbar`; the handles' refresh then shows or hides it.
    fn set_show_toolbar(&mut self, container: NodeId, show: bool) {
        if let Some(found) = self.container_handles(container) {
            found.handles.show_toolbar.set(show);
        }
    }

    /// The end a gesture moves is `start`'s (or the end's), the gesture at `position` in the
    /// root: what `container`'s magnifier follows, as Compose's `updateSelection` sets
    /// `draggingHandle` and `currentDragPosition` (D2073). Nothing where it has no magnifier.
    fn set_magnifier_drag(&mut self, container: NodeId, start: bool, position: Offset) {
        let Some(found) = self.container_handles(container) else { return };
        if found.handles.magnifier.get().is_none() {
            return;
        }
        let origin = self.layer_origin(container, found.layer);
        found.handles.set_magnifier_drag(Some((start, position - origin)));
    }

    fn apply_selection_request(&mut self, request: SelectionRequest) {
        match request {
            SelectionRequest::Start { container, position, unit } => {
                self.set_magnifier_drag(container, false, position);
                // A gesture starting hides the toolbar (`onSelectionUpdateStartCallback`).
                self.set_show_toolbar(container, false);
                let entries = self.selectables(container);
                self.clear_other_selection(container);
                let Some(Placed { point, .. }) = self.point_at(container, &entries, position, None)
                else {
                    return;
                };
                let (anchor, focus) = match unit {
                    Unit::Character => (point, point),
                    Unit::Word => {
                        let word = self.word(point);
                        (Point { offset: word.start, ..point }, Point { offset: word.end, ..point })
                    }
                    Unit::Paragraph => {
                        let paragraph = self.paragraph(point);
                        (
                            Point { offset: paragraph.start, ..point },
                            Point { offset: paragraph.end, ..point },
                        )
                    }
                };
                self.text_selection = Some(TextSelection { container, anchor, focus, unit });
                self.show_selection(&entries);
            }
            SelectionRequest::Extend { container, position } => {
                self.set_magnifier_drag(container, false, position);
                let Some(mut selection) =
                    self.text_selection.filter(|selection| selection.container == container)
                else {
                    return;
                };
                let entries = self.selectables(container);
                // The other end is the selection's start as it is, adjusted again at each move,
                // as Compose's `previousSelection.start` (D2028): a word's end on punctuation
                // goes past it as the drag crosses.
                let raw = selection.anchor;
                let Some(anchor) = Self::order_of(&entries, &raw) else {
                    self.text_selection = None;
                    self.show_selection(&entries);
                    return;
                };
                let found = self.point_at(container, &entries, position, Some(anchor.0));
                let Some(Placed { point, on, before }) = found else { return };
                let Some(at) = Self::order_of(&entries, &point) else { return };
                // Crossed when the moving end comes before the start, or at it from the gap
                // before its text (Compose's `crossStatus`: by offsets, else by slots).
                let crossed = if at == anchor { before } else { at < anchor };
                // A unit's far side from the other end, or its near side: by words, by
                // paragraphs, as Compose's `adjustToBoundaries`; from a gap between texts, the
                // point itself (off its slot).
                let snap = |tree: &mut UiTree, point: Point, far: bool| match selection.unit {
                    Unit::Character => point,
                    Unit::Word => {
                        let word = tree.word(point);
                        Point { offset: if far { word.end } else { word.start }, ..point }
                    }
                    Unit::Paragraph => {
                        let paragraph = tree.paragraph(point);
                        Point { offset: if far { paragraph.end } else { paragraph.start }, ..point }
                    }
                };
                selection.anchor = snap(self, raw, crossed);
                selection.focus = if on { snap(self, point, !crossed) } else { point };
                self.text_selection = Some(selection);
                self.show_selection(&entries);
                self.bring_dragged_into_view(container, &entries, position, point);
            }
            SelectionRequest::SelectAll { container } => {
                // Nothing to select: nothing done, as Compose's `selectAll` returns first.
                let entries = self.selectables(container);
                let (Some(first), Some(last)) = (entries.first(), entries.last()) else { return };
                let length = self.selectable_text(last.node, last.index).len();
                let (anchor, focus) = (first.point(0), last.point(length));
                self.text_selection =
                    Some(TextSelection { container, anchor, focus, unit: Unit::Character });
                self.show_selection(&entries);
                // As Compose's `selectAll` then: `showSelectionToolbar`.
                self.set_show_toolbar(container, true);
            }
            SelectionRequest::Copy { container } => {
                if self.text_selection.is_some_and(|selection| selection.container == container)
                    && let Some(text) = self.selected_text()
                {
                    self.clipboard.set_text(&text);
                }
            }
            SelectionRequest::Clear { container } => {
                // As Compose's `onRelease`: `hideSelectionToolbar`.
                self.set_show_toolbar(container, false);
                if self.text_selection.is_some_and(|selection| selection.container == container) {
                    let entries = self.selectables(container);
                    self.text_selection = None;
                    self.show_selection(&entries);
                }
            }
            SelectionRequest::LongPress { container, position, unit } => {
                // On a text only: Compose's long press is each text's (`SelectionController`),
                // in its minimum touch target too (D2029).
                let entries = self.selectables(container);
                let Some(entry) = self.touched_text(container, &entries, position) else { return };
                // Put in the text's bounds, as Compose's `onSelectionUpdateStartCallback`
                // (`coerceIn`); the end then goes where the finger is, as Compose's tests find
                // it: before the text, the selection crosses.
                let (origin, size) = (entry.origin, entry.size);
                let inside = Offset::new(
                    position.x.clamp(origin.x, origin.x + size.width as f32),
                    position.y.clamp(origin.y, origin.y + size.height as f32),
                );
                self.apply_selection_request(SelectionRequest::Start {
                    container,
                    position: inside,
                    unit,
                });
                if inside != position {
                    self.apply_selection_request(SelectionRequest::Extend { container, position });
                }
            }
            SelectionRequest::RefreshHandles { .. } => {}
            SelectionRequest::DragHandle { container, start, moved } => {
                self.drag_handle(container, start, moved)
            }
            SelectionRequest::GestureEnded { container } => {
                self.set_show_toolbar(container, true);
                // As Compose's `onSelectionUpdateEndCallback`: no gesture, no magnifier.
                if let Some(found) = self.container_handles(container) {
                    found.handles.set_magnifier_drag(None);
                }
            }
        }
    }

    /// A handle of `container` dragged by `moved` since its drag began (`None`: it ended), as
    /// Compose's `handleDragObserver`: the drag begins where the handle points, a pixel up
    /// (`getAdjustedCoordinates`), and its end follows the point under the finger by
    /// `CharacterWithWordAccelerate` across the texts (`accelerated`), never collapsing the
    /// selection.
    fn drag_handle(&mut self, container: NodeId, start: bool, moved: Option<Offset>) {
        let Some(found) = self.container_handles(container) else { return };
        let handles = found.handles;
        let Some(moved) = moved else {
            handles.end_drag();
            return;
        };
        let Some(mut selection) =
            self.text_selection.filter(|selection| selection.container == container)
        else {
            return;
        };
        let (begin, previous, previous_point) = match handles.drag.get() {
            Some((dragged, begin, previous)) if dragged == start => {
                (begin, previous, handles.drag_point.get())
            }
            _ => {
                let pointed =
                    compose_runtime::snapshots::Snapshot::without_read_observation(|| {
                        handles.position(start)
                    });
                // A hidden handle isn't dragged, as Compose's `onDown`.
                let Some(pointed) = pointed else { return };
                let begin = Offset::new(pointed.x, pointed.y - 1.0);
                (begin, begin, None)
            }
        };
        let origin = self.layer_origin(container, found.layer);
        let entries = self.selectables(container);
        let target = origin + begin + moved;
        // Compose's `currentDragPosition`, whatever the selection does.
        handles.set_magnifier_drag(Some((start, begin + moved)));
        let other = if start { selection.focus } else { selection.anchor };
        let anchor = Self::order_of(&entries, &other).map(|(order, _)| order);
        let Some(Placed { point, .. }) = self.point_at(container, &entries, target, anchor) else {
            return;
        };
        // The selection follows the finger when its place in the texts changed since it last
        // did (the texts may have moved under a still finger), as Compose's
        // `shouldRecomputeSelection`; the place it was then is the next move's previous one.
        if previous_point == Some(point) {
            handles.drag.set(Some((start, begin, previous)));
            self.bring_dragged_into_view(container, &entries, target, point);
            return;
        }
        handles.drag.set(Some((start, begin, begin + moved)));
        handles.drag_point.set(Some(point));
        let (end, other) = if start {
            (selection.anchor, selection.focus)
        } else {
            (selection.focus, selection.anchor)
        };
        let previous = (origin + previous, previous_point);
        let moved_to = self.accelerated(&entries, start, point, target, previous, end, other);
        // Never collapsed by a handle, as Compose's `ensureAtLeastOneChar` keeps a character.
        if moved_to != other {
            if start {
                selection.anchor = moved_to;
            } else {
                selection.focus = moved_to;
            }
        }
        selection.unit = Unit::Character;
        self.text_selection = Some(selection);
        self.show_selection(&entries);
        self.bring_dragged_into_view(container, &entries, target, point);
    }

    /// Scrolls what's around the text under a drag toward the finger, as Compose's
    /// `bringDraggedPositionIntoView` (`isSelectionAutoScrollEnabled`): the text whose point
    /// the drag at `position` (in the root) is at asks its scrolling ancestors for the drag's
    /// place, in touch mode with the cursor's place at the end's offset there (Compose's
    /// `getCursorRect`, D2030), stretched to the nearest point of the text's bounds, so a list scrolls
    /// a little each move, never past what it composes next.
    fn bring_dragged_into_view(
        &mut self,
        container: NodeId,
        entries: &[Entry],
        position: Offset,
        point: Point,
    ) {
        let Some(entry) = entries.iter().find(|entry| entry.holds(&point)).copied() else {
            return;
        };
        let local = position - entry.origin;
        let mut rect = Rect::new(local.x, local.y, local.x, local.y);
        if self.container_in_touch_mode(container) {
            let edge = self
                .node_and_measurer(entry.node)
                .and_then(|(value, (measurer, _))| {
                    let text = value.chain[entry.index].selectable()?;
                    Some(text.cursor_rect(point.offset, measurer))
                })
                .unwrap_or(rect);
            rect = union(rect, edge);
        }
        let (width, height) = (entry.size.width as f32, entry.size.height as f32);
        let nearest = Offset::new(local.x.clamp(0.0, width), local.y.clamp(0.0, height));
        rect = union(rect, Rect::new(nearest.x, nearest.y, nearest.x, nearest.y));
        self.bring_entry_rect_into_view(entry.node, entry.index, rect);
    }

    /// Whether `container`'s node is in touch mode (its last pointer a finger's).
    fn container_in_touch_mode(&mut self, container: NodeId) -> bool {
        let Some(value) = self.get_mut(container) else { return false };
        value.chain.iter_mut().find_map(|entry| {
            entry.node.downcast_mut::<SelectionContainerNode>().map(|node| node.touch == Some(true))
        }) == Some(true)
    }

    /// Pins the pinnable containers (a lazy layout's items) of the texts `selected` holds a
    /// part of, and lets go of those of the texts among `entries` it no longer does, as
    /// Compose's `updatePinnedSelectables`: a selected text scrolled away stays composed. A
    /// pinned text not among `entries` (not placed) keeps its pin while a selection exists.
    fn update_selection_pins(&mut self, entries: &[Entry], selected: &[NodeId]) {
        if self.text_selection.is_none() {
            self.release_selection_pins();
            return;
        }
        let mut index = 0;
        while index < self.selection_pins.len() {
            let node = self.selection_pins[index].0;
            let shown = entries.iter().any(|entry| entry.node == node);
            if shown && !selected.contains(&node) {
                self.selection_pins.swap_remove(index).1.release();
            } else {
                index += 1;
            }
        }
        for &node in selected {
            if self.selection_pins.iter().any(|(pinned, _)| *pinned == node) {
                continue;
            }
            let container = self.node(node).locals.get(&crate::pinnable::LOCAL_PINNABLE_CONTAINER);
            if let Some(container) = container {
                self.selection_pins.push((node, container.pin()));
            }
        }
    }

    /// `node` leaves (detached, deactivated or reset for reuse): its text lets go of its pin at
    /// once, as Compose's `afterSelectableUnsubscribe`, so neither an item whose selected text
    /// went nor another text its node is reused for keeps it.
    pub(crate) fn selectable_leaving(&mut self, node: NodeId) {
        if let Some(at) = self.selection_pins.iter().position(|(pinned, _)| *pinned == node) {
            self.selection_pins.swap_remove(at).1.release();
        }
    }

    /// Lets go of every selected text's pin.
    pub(crate) fn release_selection_pins(&mut self) {
        for (_, handle) in self.selection_pins.drain(..) {
            handle.release();
        }
    }

    /// Where a handle's drag moves its end (`end`, the other being `other`) when the finger is at
    /// `target` (in the root), over `point`, having been at `previous`, as Compose's
    /// `SelectionAdjustment.CharacterWithWordAccelerate`: between texts, the point; into another
    /// text or onto another line, the word under it (`snapped`); shrinking, the point; growing
    /// from a word's boundary, by words, and from inside a word, by characters.
    #[allow(clippy::too_many_arguments)]
    fn accelerated(
        &mut self,
        entries: &[Entry],
        start: bool,
        point: Point,
        target: Offset,
        previous: (Offset, Option<Point>),
        end: Point,
        other: Point,
    ) -> Point {
        let Some(entry) = entries.iter().find(|entry| entry.holds(&point)).copied() else {
            return point;
        };
        let inside = |entry: &Entry| {
            let local = target - entry.origin;
            local.x >= 0.0
                && local.y >= 0.0
                && local.x < entry.size.width as f32
                && local.y < entry.size.height as f32
        };
        // Between texts (none under the finger): the point, as `updateSelectionBoundary`.
        if !entries.iter().any(inside) {
            return point;
        }
        let order = |at: &Point| Self::order_of(entries, at);
        let crossed =
            if start { order(&point) > order(&other) } else { order(&other) > order(&point) };
        // The other end in this text, or this text's edge toward it (`rawEndHandleOffset`).
        let other_raw = if entry.holds(&other) {
            other.offset
        } else if order(&other) > order(&point) {
            self.selectable_text(point.node, point.index).len()
        } else {
            0
        };
        let line = |tree: &mut UiTree, offset: usize| tree.line_of(&entry, offset);
        let snapped = |tree: &mut UiTree| {
            // The word under the finger, kept to its line (a word broken across lines).
            let word = tree.word(point);
            let current = line(tree, point.offset);
            let word_start = if line(tree, word.start).start == current.start {
                word.start
            } else {
                current.start
            };
            let word_end =
                if line(tree, word.end).start == current.start { word.end } else { current.end };
            let offset = if word_start == other_raw {
                word_end
            } else if word_end == other_raw {
                word_start
            } else if start != crossed {
                if point.offset <= word_end { word_start } else { word_end }
            } else if point.offset >= word_start {
                word_end
            } else {
                word_start
            };
            Point { offset, ..point }
        };
        if !entry.holds(&end) {
            // Into another text: by words.
            return snapped(self);
        }
        // Where the finger was in this text when the selection last followed it, as the previous
        // layout says it, else as its place there now says it.
        let previous_raw = match previous {
            (_, Some(point)) if entry.holds(&point) => point.offset,
            (position, _) => self.offset_in(&entry, position),
        };
        if point.offset == previous_raw {
            return end;
        }
        if line(self, previous_raw).start != line(self, point.offset).start {
            return snapped(self);
        }
        let expanding = if start != crossed {
            point.offset < previous_raw
        } else {
            point.offset > previous_raw
        };
        if !expanding {
            return point;
        }
        let word = self.word(end);
        if end.offset == word.start || end.offset == word.end { snapped(self) } else { point }
    }

    /// The line around `offset` in `entry`'s text, as `getLineForOffset` and its bounds.
    fn line_of(&mut self, entry: &Entry, offset: usize) -> Range<usize> {
        self.node_and_measurer(entry.node)
            .and_then(|(value, (measurer, _))| {
                let text = value.chain[entry.index].selectable()?;
                Some(text.range_of_line_containing(offset, measurer))
            })
            .unwrap_or(offset..offset)
    }

    /// `container`'s node, if it has handles: they, its touch mode and focus, its chain index
    /// and layer.
    fn container_handles(&mut self, container: NodeId) -> Option<FoundHandles> {
        let value = self.get(container)?;
        let mut layer = 0;
        for (index, entry) in value.chain.iter().enumerate() {
            if let Some(node) = entry.node.downcast_ref::<SelectionContainerNode>() {
                let handles = node.handles.clone()?;
                let touch = node.touch.unwrap_or_else(|| {
                    self.input_mode.input_mode() == crate::input_mode::InputMode::Touch
                });
                return Some(FoundHandles { handles, touch, focused: node.focused, index, layer });
            }
            if entry.kinds.any(NodeKinds::LAYOUT) {
                layer += 1;
            }
        }
        None
    }

    /// Tells `container`'s handles which show and where, as Compose's `SelectionContainer`'s
    /// condition and `SelectionManager.updateHandleOffsets`: in touch mode, with the focus, over
    /// a selection not trivially collapsed; each where its text says (`getHandlePosition`), in
    /// the container's coordinates, hidden outside its visible bounds unless it's dragged.
    /// Nothing for a container without handles.
    pub(crate) fn refresh_handles(&mut self, container: NodeId) {
        self.handles_placed_for = self.text_selection.and_then(|it| self.handles_placed(&it));
        let Some(found) = self.container_handles(container) else { return };
        let handles = found.handles.clone();
        handles.container.set(Some(container));
        let selection = self.text_selection.filter(|selection| {
            selection.container == container && selection.anchor != selection.focus
        });
        let mut shown = None;
        let mut positions = [None, None];
        if let Some(selection) = selection.filter(|_| found.touch && found.focused) {
            let entries = self.selectables(container);
            if let (Some(anchor), Some(focus)) = (
                Self::order_of(&entries, &selection.anchor),
                Self::order_of(&entries, &selection.focus),
            ) {
                let crossed = anchor > focus;
                let origin = self.layer_origin(container, found.layer);
                // Compose's `visibleBounds()`: clipped by the clips around it and the window.
                let clipped = self.coordinate_values(container, found.layer, found.index);
                let window = self.window_bounds_of(container);
                let visible = crate::layout_callbacks::intersect(clipped.clipped_bounds, window)
                    .translate(-clipped.position_in_root);
                let dragging = handles.dragging();
                let mut directions = [ResolvedTextDirection::Ltr; 2];
                for (slot, (point, start)) in
                    [(selection.anchor, true), (selection.focus, false)].into_iter().enumerate()
                {
                    let Some(entry) = entries.iter().find(|entry| entry.holds(&point)) else {
                        continue;
                    };
                    let asked =
                        self.node_and_measurer(entry.node).and_then(|(value, (measurer, _))| {
                            let text = value.chain[entry.index].selectable()?;
                            let position =
                                text.handle_position(point.offset, start, crossed, measurer);
                            Some((position, text.direction_at(point.offset, measurer)))
                        });
                    let Some((position, direction)) = asked else { continue };
                    directions[slot] = direction;
                    let Some(position) = position else { continue };
                    let position = position + entry.origin - origin;
                    if dragging == Some(start) || contains_inclusive(visible, position) {
                        positions[slot] = Some(position);
                    }
                }
                shown = Some(ContainerHandlesShown {
                    start_direction: directions[0],
                    end_direction: directions[1],
                    crossed,
                });
            }
        }
        let (was_shown, was_at) =
            compose_runtime::snapshots::Snapshot::without_read_observation(|| {
                (handles.shown.get(), handles.positions.get())
            });
        if was_shown != shown {
            handles.shown.set(shown);
        }
        if was_at != positions {
            handles.positions.set(positions);
            // Their popups are placed where they point after a layout: a new selection where
            // nothing moved asks for one, as Compose's popups follow their position provider.
            self.invalidate_placement_of(container);
        }
        self.refresh_toolbar(container, &found);
        self.refresh_magnifier(container, &found);
    }

    /// Tells `container`'s magnifier whether it shows and where, as Compose's
    /// `shouldShowMagnifier` and `calculateSelectionMagnifierCenterAndroid`: nothing where the
    /// platform has none, nor while no gesture moves an end and none showed (D2073).
    fn refresh_magnifier(&mut self, container: NodeId, found: &FoundHandles) {
        let Some(magnifier) = found.handles.magnifier.get() else { return };
        let drag = magnifier.drag.get();
        let was_center = compose_runtime::snapshots::Snapshot::without_read_observation(|| {
            magnifier.center.get()
        });
        if drag.is_none() && was_center.is_unspecified() {
            return;
        }
        let selection = self.text_selection.filter(|selection| selection.container == container);
        let collapsed = selection.is_none_or(|selection| selection.anchor == selection.focus);
        let shown = drag.is_some() && found.touch && !collapsed;
        let center = match (drag, selection) {
            (Some((start, at)), Some(selection)) if shown => self
                .container_magnifier_center(container, found, &selection, start, at)
                .unwrap_or(Offset::UNSPECIFIED),
            _ => Offset::UNSPECIFIED,
        };
        let same = (was_center.is_unspecified() && center.is_unspecified()) || was_center == center;
        if !same {
            magnifier.center.set(center);
        }
    }

    /// Where `container`'s magnifier is centered while the end `start` (or the end's) is moved
    /// by a gesture at `drag` (in the container's coordinates), as Compose's `getMagnifierCenter`:
    /// on its text's line, following the gesture across it but kept within the line, half way
    /// down it; none past what the text shows, or with the gesture more than half the magnifier's
    /// width off the line.
    fn container_magnifier_center(
        &mut self,
        container: NodeId,
        found: &FoundHandles,
        selection: &TextSelection,
        start: bool,
        drag: Offset,
    ) -> Option<Offset> {
        let point = if start { selection.anchor } else { selection.focus };
        let entries = self.selectables(container);
        let entry = *entries.iter().find(|entry| entry.holds(&point))?;
        let origin = self.layer_origin(container, found.layer);
        let size = found.handles.magnifier.get().map_or(IntSize::ZERO, |it| it.size.get());
        let (value, (measurer, _)) = self.node_and_measurer(entry.node)?;
        let text = value.chain[entry.index].selectable()?;
        let offset = point.offset;
        if offset > text.last_visible_offset(measurer) {
            return None;
        }
        // The gesture's place across the line follows it, not the selection's offsets.
        let drag_x = drag.x + origin.x - entry.origin.x;
        let line = text.range_of_line_containing(offset, measurer);
        let x = if line.is_empty() {
            // An empty line: its left and right are one.
            text.line_left(offset, measurer)?
        } else {
            let string = text.text();
            let mut last = line.end - 1;
            while !string.as_str().is_char_boundary(last) {
                last -= 1;
            }
            let left = text.line_left(line.start, measurer)?;
            let right = text.line_right(last, measurer)?;
            // Left to right or right to left, either may be the larger.
            drag_x.clamp(left.min(right), left.max(right))
        };
        if size != IntSize::ZERO && (drag_x - x).abs() > (size.width / 2) as f32 {
            return None;
        }
        let y = text.center_y_for_offset(offset, measurer)?;
        Some(entry.origin - origin + Offset::new(x, y))
    }

    /// Shows or hides `container`'s toolbar, as Compose's `updateSelectionToolbar`: shown while
    /// it's asked for (`showToolbar`) in touch mode with the focus and its rect is visible, hidden
    /// otherwise; its rect a state the toolbar follows. Nothing while it's neither asked for nor
    /// shown (D2041).
    fn refresh_toolbar(&mut self, container: NodeId, found: &FoundHandles) {
        let handles = &found.handles;
        let asked = handles.show_toolbar.get() && found.touch && found.focused;
        if !asked && !handles.toolbar_requested.get() {
            return;
        }
        let rect = if asked { self.container_toolbar_rect(container, found) } else { None };
        if let Some(rect) = rect {
            let states = handles.toolbar_states();
            let was = compose_runtime::snapshots::Snapshot::without_read_observation(|| {
                states.content_rect.get()
            });
            if was != Some(rect) {
                states.content_rect.set(Some(rect));
            }
        }
        let show = rect.is_some();
        if !show && !handles.toolbar_requested.get() {
            return;
        }
        handles.toolbar_requested.set(show);
        // Shown (again, if it was closed) or hidden from a task, outside the tree's work, as
        // Compose's requester is told at each update.
        let Some(requests) = handles.toolbar_requests.borrow().clone() else { return };
        compose_runtime::platform::launch(async move { requests(show) });
    }

    /// Where `container`'s toolbar goes, in the root, as Compose's `getContentRect`: the region
    /// of the selection in the first and the last text it covers (`getSelectedRegionRect` over
    /// `firstAndLast`), kept to the container's visible bounds (touching them is enough), its
    /// bottom four handles' heights lower (`HandleHeight.value * 4`: 100 px whatever the
    /// density, as Compose's). `None` without a selection of some characters there, or out of
    /// view.
    fn container_toolbar_rect(&mut self, container: NodeId, found: &FoundHandles) -> Option<Rect> {
        let selection = self.text_selection.filter(|selection| selection.container == container)?;
        let entries = self.selectables(container);
        let span = Self::span(&selection, &entries)?;
        let mut covered = (0..entries.len()).filter(|&order| range_of(span, order).is_some());
        let first = covered.next()?;
        let last = covered.next_back().unwrap_or(first);
        let origin = self.layer_origin(container, found.layer);
        let orders: &[usize] = if first == last { &[first] } else { &[first, last] };
        let region = self.region_of(&entries, span, origin, orders)?;
        let clipped = self.coordinate_values(container, found.layer, found.index);
        let window = self.window_bounds_of(container);
        let visible = crate::layout_callbacks::intersect(clipped.clipped_bounds, window)
            .translate(-clipped.position_in_root);
        let rect = Rect::new(
            visible.left.max(region.left),
            visible.top.max(region.top),
            visible.right.min(region.right),
            visible.bottom.min(region.bottom),
        );
        if rect.right < rect.left || rect.bottom < rect.top {
            return None;
        }
        let root = rect.translate(clipped.position_in_root);
        Some(Rect::new(root.left, root.top, root.right, root.bottom + HANDLE_HEIGHT * 4.0))
    }

    /// Which of `container`'s menu items show, as Compose's `isNonEmptySelection` and
    /// `isEntireContainerSelected`: copy when the selection there covers some characters, select
    /// all unless every text that has some is wholly selected.
    fn container_menu_items(&mut self, container: NodeId) -> ContainerMenuItems {
        let entries = self.selectables(container);
        let span = self
            .text_selection
            .filter(|selection| selection.container == container)
            .and_then(|selection| Self::span(&selection, &entries));
        let mut items = ContainerMenuItems::default();
        for (order, entry) in entries.iter().enumerate() {
            let text = self.selectable_text(entry.node, entry.index);
            let range = span.and_then(|span| range_of(span, order)).map(|it| clamp(&text, it));
            if range.as_ref().is_some_and(|range| !range.is_empty()) {
                items.copy = true;
            }
            if !text.is_empty() && range.is_none_or(|range| range.len() != text.len()) {
                items.select_all = true;
            }
        }
        items
    }

    /// After a layout or graphics layers moving (`moved`), tells `selection`'s handles where they
    /// point if its container or the texts of its ends moved or changed size, as Compose's
    /// container updates them when its position changes and a selected text when its
    /// coordinates do; else nothing.
    pub(crate) fn refresh_handles_if_moved(&mut self, selection: &TextSelection, moved: &[NodeId]) {
        let nodes = [selection.container, selection.anchor.node, selection.focus.node];
        let layer_moved =
            moved.iter().any(|&layer| nodes.iter().any(|&node| self.is_at_or_below(node, layer)));
        if layer_moved || self.handles_placed(selection) != self.handles_placed_for {
            self.refresh_handles(selection.container);
        }
    }

    /// The selection's container and the texts of its ends map through the graphics layers
    /// around them again, so those layers tell their moves to `refresh_handles_if_moved` (after
    /// `position_listeners` moved, D2105).
    pub(crate) fn note_selection_layers(&self) {
        let Some(selection) = self.text_selection else { return };
        for node in [selection.container, selection.anchor.node, selection.focus.node] {
            if let Some(value) = self.get(node) {
                let layer = value.layers.len() - 1;
                self.map_to_root(node, layer, None, Offset::ZERO);
            }
        }
    }

    /// Where `selection`'s container and the texts of its ends are now, in the root, with their
    /// sizes (graphics layers left out: their moves are told apart).
    fn handles_placed(&self, selection: &TextSelection) -> Option<HandlesPlacedFor> {
        let at = |node: NodeId| {
            let value = self.get(node)?;
            Some((node, self.layer_origin(node, 0), value.layers.first()?.size))
        };
        Some(HandlesPlacedFor([
            at(selection.container)?,
            at(selection.anchor.node)?,
            at(selection.focus.node)?,
        ]))
    }

    /// A selection started in another container clears this one's.
    fn clear_other_selection(&mut self, container: NodeId) {
        if let Some(old) = self.text_selection
            && old.container != container
        {
            self.text_selection = None;
            if self.get(old.container).is_some() {
                let entries = self.selectables(old.container);
                self.show_selection(&entries);
            } else {
                self.release_selection_pins();
            }
        }
    }

    /// Tells each text of `entries` its part of the selection (dropping a selection whose
    /// texts are gone or changed).
    fn show_selection(&mut self, entries: &[Entry]) {
        let span = self.text_selection.and_then(|selection| Self::span(&selection, entries));
        if span.is_none() {
            self.text_selection = None;
        }
        let mut selected: SmallVec<[NodeId; 8]> = SmallVec::new();
        for (order, entry) in entries.iter().enumerate() {
            let range = span.and_then(|span| range_of(span, order));
            let text = self.selectable_text(entry.node, entry.index);
            let range = range.map(|range| clamp(&text, range));
            if range.as_ref().is_some_and(|range| range.start < range.end) {
                selected.push(entry.node);
            }
            let changed =
                self.node_and_measurer(entry.node).is_some_and(|(value, (measurer, _))| {
                    value.chain[entry.index]
                        .selectable()
                        .is_some_and(|text| text.set_selection(range, measurer))
                });
            if changed {
                self.invalidate_draw_of(entry.node);
            }
        }
        if !selected.is_empty() || !self.selection_pins.is_empty() {
            self.update_selection_pins(entries, &selected);
        }
    }

    /// The bounds of what's selected in `container`, in its coordinates, as Compose's
    /// `getSelectedRegionRect` (where the text toolbar goes): for each text the selection covers
    /// a part of, its first and last selected characters' boxes, all together. `None` without a
    /// selection there, or with a collapsed one.
    #[doc(hidden)]
    pub fn selected_region_rect(&mut self, container: NodeId) -> Option<Rect> {
        let selection = self.text_selection.filter(|selection| selection.container == container)?;
        let mut layer = 0;
        for entry in self.get(container)?.chain.iter() {
            if entry.node.downcast_ref::<SelectionContainerNode>().is_some() {
                break;
            }
            if entry.kinds.any(NodeKinds::LAYOUT) {
                layer += 1;
            }
        }
        let origin = self.layer_origin(container, layer);
        let entries = self.selectables(container);
        let span = Self::span(&selection, &entries)?;
        let orders: Vec<usize> = (0..entries.len()).collect();
        self.region_of(&entries, span, origin, &orders)
    }

    /// The region `span` selects in the texts of `entries` at `orders`, in the coordinates of
    /// what's at `origin`: each one's first and last selected characters' boxes, all together, as
    /// Compose's `getSelectedRegionRect`. `None` if none has selected characters.
    fn region_of(
        &mut self,
        entries: &[Entry],
        span: Span,
        origin: Offset,
        orders: &[usize],
    ) -> Option<Rect> {
        let mut region: Option<Rect> = None;
        for &order in orders {
            let entry = &entries[order];
            let Some(range) = range_of(span, order) else { continue };
            let text = self.selectable_text(entry.node, entry.index);
            let range = clamp(&text, range);
            if range.is_empty() {
                continue;
            }
            let last = text[..range.end].char_indices().next_back().map_or(0, |(at, _)| at);
            let Some((value, (measurer, _))) = self.node_and_measurer(entry.node) else { continue };
            let Some(selectable) = value.chain[entry.index].selectable() else { continue };
            for offset in [range.start, last] {
                let found =
                    selectable.bounding_box(offset, measurer).translate(entry.origin - origin);
                region = Some(match region {
                    Some(region) => Rect::new(
                        region.left.min(found.left),
                        region.top.min(found.top),
                        region.right.max(found.right),
                        region.bottom.max(found.bottom),
                    ),
                    None => found,
                });
            }
        }
        region
    }

    /// Runs `ask` with the `Selectable` of `node` (its first), if it has one. For tests.
    #[doc(hidden)]
    pub fn with_selectable(
        &mut self,
        node: NodeId,
        ask: &mut dyn FnMut(&mut dyn Selectable, &SharedTextMeasurer),
    ) {
        let Some((value, (measurer, _))) = self.node_and_measurer(node) else { return };
        for entry in value.chain.iter_mut() {
            if let Some(selectable) = entry.selectable() {
                ask(selectable, measurer);
                return;
            }
        }
    }

    /// Where the selection starts and ends: each end's text node and offset, its start handle's
    /// first, as Compose's `Selection.start` and `end` (`None` without a selection). For tests.
    #[doc(hidden)]
    pub fn selection_anchors(&self) -> Option<((NodeId, usize), (NodeId, usize))> {
        let selection = self.text_selection?;
        let (anchor, focus) = (selection.anchor, selection.focus);
        Some(((anchor.node, anchor.offset), (focus.node, focus.offset)))
    }

    /// The direction of the text at the selection's start and end, as Compose's
    /// `Selection.AnchorInfo.direction` (`None` without a selection, or with an end whose text is
    /// gone or changed). For tests.
    #[doc(hidden)]
    pub fn selection_directions(
        &mut self,
    ) -> Option<(ResolvedTextDirection, ResolvedTextDirection)> {
        let selection = self.text_selection?;
        let entries = self.selectables(selection.container);
        let mut direction = |point: Point| {
            let entry = entries.iter().find(|entry| entry.holds(&point))?;
            let (value, (measurer, _)) = self.node_and_measurer(entry.node)?;
            let text = value.chain[entry.index].selectable()?;
            Some(text.direction_at(point.offset, measurer))
        };
        Some((direction(selection.anchor)?, direction(selection.focus)?))
    }

    /// The text selected in a container, if any (what Cmd+C copies there): `None` when nothing
    /// is, or a text it ends in is gone or changed.
    pub fn selected_text(&mut self) -> Option<String> {
        let selection =
            self.text_selection.filter(|selection| selection.anchor != selection.focus)?;
        let entries = self.selectables(selection.container);
        let Some(span) = Self::span(&selection, &entries) else {
            self.text_selection = None;
            self.show_selection(&entries);
            return None;
        };
        let mut parts = Vec::new();
        for (order, entry) in entries.iter().enumerate() {
            if let Some(range) = range_of(span, order) {
                let text = self.selectable_text(entry.node, entry.index);
                parts.push(text[clamp(&text, range)].to_string());
            }
        }
        Some(parts.join("\n"))
    }
}

/// A container's handles, as `container_handles` finds them.
struct FoundHandles {
    handles: Rc<ContainerHandles>,
    touch: bool,
    focused: bool,
    index: usize,
    layer: usize,
}

/// `rect` holds `point`, its edges included, as Compose's `containsInclusive`.
fn contains_inclusive(rect: Rect, point: Offset) -> bool {
    point.x >= rect.left && point.x <= rect.right && point.y >= rect.top && point.y <= rect.bottom
}

impl SelectionRequest {
    fn container(&self) -> NodeId {
        match *self {
            SelectionRequest::Start { container, .. }
            | SelectionRequest::Extend { container, .. }
            | SelectionRequest::SelectAll { container }
            | SelectionRequest::Copy { container }
            | SelectionRequest::Clear { container }
            | SelectionRequest::LongPress { container, .. }
            | SelectionRequest::RefreshHandles { container }
            | SelectionRequest::DragHandle { container, .. }
            | SelectionRequest::GestureEnded { container } => container,
        }
    }
}

/// The part of the text at `order` a selection spanning `span` covers, if any.
fn range_of(span: Span, order: usize) -> Option<Range<usize>> {
    let (start, end) = span;
    if start == end || order < start.0 || order > end.0 {
        return None;
    }
    let from = if order == start.0 { start.1 } else { 0 };
    let to = if order == end.0 { end.1 } else { usize::MAX };
    Some(from..to)
}

/// The smallest rectangle holding both, as Compose's `Rect.union`.
fn union(a: Rect, b: Rect) -> Rect {
    Rect::new(a.left.min(b.left), a.top.min(b.top), a.right.max(b.right), a.bottom.max(b.bottom))
}

/// `range` within `text`, at character boundaries.
fn clamp(text: &str, range: Range<usize>) -> Range<usize> {
    let floor = |offset: usize| {
        let mut offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    };
    floor(range.start)..floor(range.end)
}

// ---- The container's node ------------------------------------------------------------------

/// Compose's `HandleHeight` (25 dp) as its `value`: the toolbar's rect reaches four of them below
/// the selection, in pixels at any density, as Compose's `getContentRect`.
const HANDLE_HEIGHT: f32 = 25.0;

/// A press within this long of the last, and this close to it, is a double- or triple-click.
const MULTI_CLICK_NANOS: u64 = 500_000_000;
const MULTI_CLICK_DISTANCE: f32 = 8.0;

#[derive(Clone)]
struct SelectionContainerElement(Option<Rc<ContainerHandles>>);

/// `SelectionContainer`'s node: it turns presses, drags and keys into selection requests, and
/// takes the focus a press gives it (Cmd+C reaches it). A mouse selects as it presses and drags;
/// a finger's long press selects a word and its drag extends by words, a finger's tap clears
/// the selection (Compose's `SelectionController` and `onClearSelectionRequested`).
pub(crate) struct SelectionContainerNode {
    /// A press is selecting.
    dragging: bool,
    focused: bool,
    /// When and where the last press was, and how many came in a row.
    last_press: Option<(u64, Offset, u32)>,
    /// The handles foundation's `SelectionContainer` composes, if any.
    handles: Option<Rc<ContainerHandles>>,
    /// The last pointer was a finger's (or a stylus'), as `SelectionManager.isInTouchMode`
    /// (`updateSelectionTouchMode`); the host's input mode until a pointer comes.
    touch: Option<bool>,
    /// A finger's press: where it went down, its long-press timer, whether it became one.
    finger: Option<FingerPress>,
    /// The last finger's down, when (the event's time) and where, and how many came in a row,
    /// as Compose's `ClicksCounter`.
    last_down: Option<(u64, Offset, u32)>,
}

struct FingerPress {
    /// The finger pressing: a long press's drag follows it alone, as Compose's
    /// `drag(pointerId)`.
    pointer: crate::input::PointerId,
    at: Offset,
    job: Option<compose_runtime::Job>,
    /// A long press, or a later press of a multi-tap: its moves drag the selection.
    long: bool,
    /// A later press of a multi-tap whose finger hasn't passed the touch slop yet, nor been held
    /// past the long-press timeout (Compose's `touchSelectionSubsequentPress`).
    slop_pending: bool,
    /// What the drag leaves out of the finger's place: the touch slop a later press passed
    /// (Compose's over-slop).
    slop: Offset,
    /// Where the finger last was: a move that doesn't move it drags nothing, as Compose's
    /// `drag(pointerId)` hears only moves that change its place (D2028).
    last: Offset,
}

impl SelectionContainerNode {
    fn end_finger(&mut self) {
        if let Some(job) = self.finger.take().and_then(|finger| finger.job) {
            job.cancel();
        }
    }
}

impl ModifierNodeElement for SelectionContainerElement {
    type Node = SelectionContainerNode;
    fn create(&self) -> SelectionContainerNode {
        SelectionContainerNode {
            dragging: false,
            focused: false,
            last_press: None,
            handles: self.0.clone(),
            touch: None,
            finger: None,
            last_down: None,
        }
    }
    fn update(&self, node: &mut SelectionContainerNode) {
        node.handles = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
    fn name(&self) -> &'static str {
        "selection_container"
    }
}

impl ModifierNode for SelectionContainerNode {
    /// Reused: no press selecting, no focus, no click run (the tree drops its selection).
    fn on_reset(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.dragging = false;
        self.focused = false;
        self.last_press = None;
        self.touch = None;
        self.last_down = None;
        self.end_finger();
    }
    /// Its handles learn the tree, the container and its coordinates, which its menu's items and
    /// toolbar use.
    fn on_attach(&mut self, context: &mut crate::node_context::NodeContext<'_>) {
        if let Some(handles) = &self.handles {
            handles.tree.set(Some(context.tree.key));
            handles.container.set(Some(context.node));
            handles.coordinates.set(Some(context.require_layout_coordinates()));
        }
    }
    fn on_detach(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.end_finger();
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for SelectionContainerNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Initial {
            // A long press's drag is the selection's before what's inside sees it, as Compose's
            // texts take it ahead of the scrollables around them (a lazy column's items).
            let dragging = self.finger.as_mut().filter(|finger| {
                finger.long && !finger.slop_pending && finger.pointer == event.pointer
            });
            if let Some(finger) = dragging
                && event.kind_moving() == PointerEventType::Move
                && event.pressed
                && !event.consumed
            {
                let Some(position) = context.pointer_position() else { return };
                event.consume();
                if std::mem::replace(&mut finger.last, position) == position {
                    return;
                }
                let position = position - finger.slop;
                let container = context.node;
                context.requests.selection.push(SelectionRequest::Extend { container, position });
            }
            return;
        }
        if pass != PointerEventPass::Main {
            return;
        }
        let container = context.node;
        let Some(position) = context.pointer_position() else { return };
        // As Compose's `updateSelectionTouchMode`: each event says whether it's a finger's.
        let touch = event.pointer_type != crate::input::PointerType::Mouse;
        if self.touch != Some(touch) {
            self.touch = Some(touch);
            if self.handles.is_some() {
                context.requests.selection.push(SelectionRequest::RefreshHandles { container });
            }
        }
        if touch {
            self.on_finger_event(event, position, context);
            return;
        }
        match event.kind_moving() {
            PointerEventType::Press => {
                if event.consumed {
                    // A clickable inside took it: the selection goes, as Compose's tap.
                    context.requests.selection.push(SelectionRequest::Clear { container });
                    return;
                }
                let now = compose_runtime::platform::now_nanos();
                let clicks = match self.last_press {
                    Some((at, where_, count))
                        if now.saturating_sub(at) <= MULTI_CLICK_NANOS
                            && (where_ - position).x.hypot((where_ - position).y)
                                <= MULTI_CLICK_DISTANCE =>
                    {
                        count + 1
                    }
                    _ => 1,
                };
                self.last_press = Some((now, position, clicks));
                let unit = match clicks {
                    1 => Unit::Character,
                    2 => Unit::Word,
                    _ => Unit::Paragraph,
                };
                self.dragging = true;
                context.request_focus();
                context.requests.selection.push(SelectionRequest::Start {
                    container,
                    position,
                    unit,
                });
            }
            PointerEventType::Move if self.dragging && event.pressed && !event.consumed => {
                event.consume();
                context.requests.selection.push(SelectionRequest::Extend { container, position });
            }
            // Its drag done (`onDragDone`): the toolbar may show.
            PointerEventType::Release if std::mem::replace(&mut self.dragging, false) => {
                context.requests.selection.push(SelectionRequest::GestureEnded { container });
            }
            _ => {}
        }
    }

    /// A press selecting, cancelled, ends as Compose's `onCancel` and `onDragDone` do: the
    /// toolbar may show, told from a task (no event's context here).
    fn on_cancel_pointer_input(&mut self) {
        let selecting = std::mem::replace(&mut self.dragging, false)
            || self.finger.as_ref().is_some_and(|finger| finger.long);
        self.end_finger();
        if selecting && let Some(handles) = &self.handles {
            handles.handle_released();
        }
    }
}

impl SelectionContainerNode {
    /// A finger's event, as Compose's `touchSelectionFirstPress` over the texts and the
    /// container's `onClearSelectionRequested`: held past the long-press timeout without moving
    /// past the touch slop, it selects the word under it and its drag extends by words; lifted
    /// before, it's a tap, which clears the selection.
    fn on_finger_event(
        &mut self,
        event: &mut PointerEvent,
        position: Offset,
        context: &mut EventContext<'_>,
    ) {
        let container = context.node;
        // Another finger of the gesture under way isn't its.
        if self.finger.as_ref().is_some_and(|finger| finger.pointer != event.pointer) {
            return;
        }
        match event.kind_moving() {
            PointerEventType::Press => {
                self.end_finger();
                // A mouse's press still selecting ends as the finger comes, as Android cancels it.
                if std::mem::replace(&mut self.dragging, false) {
                    context.requests.selection.push(SelectionRequest::GestureEnded { container });
                }
                if event.consumed {
                    context.requests.selection.push(SelectionRequest::Clear { container });
                    return;
                }
                // As Compose's `ClicksCounter`: a down soon after the last and near it is the
                // next of a multi-tap.
                let configuration = context.view_configuration();
                let clicks = match self.last_down {
                    Some((time, at, clicks))
                        if event.time_nanos.saturating_sub(time)
                            < configuration.double_tap_timeout_millis * 1_000_000
                            && (position - at).x.hypot((position - at).y)
                                < configuration.touch_slop =>
                    {
                        clicks + 1
                    }
                    _ => 1,
                };
                self.last_down = Some((event.time_nanos, position, clicks));
                let later = clicks > 1;
                if later {
                    // Compose's `touchSelectionSubsequentPress`: its word (or its paragraph, for
                    // a third tap) at once, on a text; dragged once past the slop or held.
                    let unit = if clicks == 2 { Unit::Word } else { Unit::Paragraph };
                    context.request_focus();
                    context.requests.selection.push(SelectionRequest::LongPress {
                        container,
                        position,
                        unit,
                    });
                }
                let handle = context.handle();
                let job = handle.coroutine_scope().map(|scope| {
                    let timeout = configuration.long_press_timeout_millis;
                    scope.launch(async move {
                        compose_runtime::delay(std::time::Duration::from_millis(timeout)).await;
                        let key = handle.tree_key();
                        crate::node_context::with_free_tree(key, &mut |tree| {
                            tree.container_long_pressed(container);
                        });
                    })
                });
                let pointer = event.pointer;
                self.finger = Some(FingerPress {
                    pointer,
                    at: position,
                    job,
                    long: later,
                    slop_pending: later,
                    slop: Offset::ZERO,
                    last: position,
                });
            }
            PointerEventType::Move if event.pressed => {
                let Some(finger) = &mut self.finger else { return };
                if finger.slop_pending {
                    // A later press's drag starts past the slop, the slop left out of it.
                    let moved = position - finger.at;
                    let distance = moved.x.hypot(moved.y);
                    let slop = context.view_configuration().touch_slop;
                    if !event.consumed && distance > slop {
                        finger.slop_pending = false;
                        finger.slop = moved * (slop / distance);
                        if let Some(job) = finger.job.take() {
                            job.cancel();
                        }
                        event.consume();
                        finger.last = position;
                        let position = position - finger.slop;
                        context
                            .requests
                            .selection
                            .push(SelectionRequest::Extend { container, position });
                    }
                } else if !finger.long {
                    // A long press's moves were taken in the initial pass.
                    let moved = position - finger.at;
                    let slop = context.view_configuration().touch_slop;
                    if event.consumed || moved.x.hypot(moved.y) > slop {
                        self.end_finger();
                    }
                }
            }
            PointerEventType::Release => {
                let Some(finger) = self.finger.take() else { return };
                if let Some(job) = finger.job {
                    job.cancel();
                }
                if finger.long {
                    // The long press's up is its, as Compose's drag consumes it; its end may show
                    // the toolbar (`onStop`).
                    event.consume();
                    context.requests.selection.push(SelectionRequest::GestureEnded { container });
                } else if !event.consumed {
                    context.requests.selection.push(SelectionRequest::Clear { container });
                }
            }
            _ => {}
        }
    }
}

impl UiTree {
    /// A finger held still on `container` past the long-press timeout: its press becomes a long
    /// press, selecting the word under it.
    fn container_long_pressed(&mut self, container: NodeId) {
        let Some(value) = self.get_mut(container) else { return };
        let mut at = None;
        for entry in value.chain.iter_mut() {
            if let Some(node) = entry.node.downcast_mut::<SelectionContainerNode>() {
                if let Some(finger) = node.finger.as_mut() {
                    finger.job = None;
                    if finger.long {
                        // A later press held past the timeout drags without the slop, as
                        // Compose's `DownResolution.Timeout`.
                        finger.slop_pending = false;
                    } else {
                        finger.long = true;
                        at = Some(finger.at);
                    }
                }
                break;
            }
        }
        if let Some(position) = at {
            let unit = Unit::Word;
            self.handle_selection(SelectionRequest::LongPress { container, position, unit });
            // The container takes the focus as its selection starts (`startSelection`).
            if self.text_selection.is_some_and(|selection| selection.container == container) {
                let direction = crate::focus_manager::FocusDirection::Enter;
                self.request_focus_at_or_below(container, direction);
            }
        }
    }
}

impl FocusTargetModifierNode for SelectionContainerNode {
    fn on_focus_changed(
        &mut self,
        _previous: crate::focus::FocusState,
        current: crate::focus::FocusState,
        context: &mut EventContext<'_>,
    ) {
        let focused = current.has_focus();
        if self.focused && !focused {
            context.requests.selection.push(SelectionRequest::Clear { container: context.node });
        } else if !self.focused && focused && self.handles.is_some() {
            let container = context.node;
            context.requests.selection.push(SelectionRequest::RefreshHandles { container });
        }
        self.focused = focused;
    }
}

impl KeyInputModifierNode for SelectionContainerNode {
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        let shortcut =
            if cfg!(target_os = "macos") { event.modifiers.meta } else { event.modifiers.ctrl };
        if event.kind != KeyEventType::KeyDown || !shortcut {
            return false;
        }
        let container = context.node;
        let request = match event.key {
            Key::Character(c) if c.eq_ignore_ascii_case(&'c') => {
                SelectionRequest::Copy { container }
            }
            Key::Character(c) if c.eq_ignore_ascii_case(&'a') => {
                SelectionRequest::SelectAll { container }
            }
            _ => return false,
        };
        context.requests.selection.push(request);
        true
    }
}

/// `DisableSelection`'s node: a mark the container's search stops at.
pub(crate) struct DisableSelectionNode;

struct DisableSelectionElement;

impl ModifierNodeElement for DisableSelectionElement {
    type Node = DisableSelectionNode;
    fn create(&self) -> DisableSelectionNode {
        DisableSelectionNode
    }
    fn update(&self, _node: &mut DisableSelectionNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "disable_selection"
    }
}

impl ModifierNode for DisableSelectionNode {}

impl Modifier {
    /// Makes this node's texts selectable, as `SelectionContainer` does for its content.
    pub fn selection_container(self) -> Modifier {
        self.then_custom(SelectionContainerElement(None))
    }

    /// A selection container whose handles foundation's `SelectionContainer` composes, told
    /// through `handles` (`SelectionManager`'s handle states).
    #[doc(hidden)]
    pub fn selection_container_with_handles(self, handles: Rc<ContainerHandles>) -> Modifier {
        self.then_custom(SelectionContainerElement(Some(handles)))
    }

    /// Leaves this node's texts out of a selection container, as `DisableSelection`.
    pub fn disable_selection(self) -> Modifier {
        self.then_custom(DisableSelectionElement)
    }
}

impl TextSelection {
    /// The container it's in.
    pub(crate) fn container(&self) -> NodeId {
        self.container
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A container's menu action asked while its tree is busy would do nothing and close the
    /// menu: a bug in what calls it, loud in a debug build.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "while its tree was busy")]
    fn a_menu_action_asked_while_the_tree_is_busy_is_a_bug() {
        let host = crate::host::UiHost::new(
            IntSize::new(10, 10),
            crate::unit::Density::new(1.0),
            Box::new(crate::fixed_text::FixedTextMeasurer::new()),
        );
        host.set_content(|| {});
        let handles = ContainerHandles::new();
        host.with_tree(|tree| {
            handles.tree.set(Some(tree.key));
            handles.container.set(Some(tree.root_id()));
            handles.run_menu_action(ContainerMenuAction::Copy);
        });
    }

    /// Select all over a container with nothing to select does nothing, as Compose's
    /// `selectAll()` returns before `showSelectionToolbar()` and its focus request: no toolbar
    /// asked for, no focus taken.
    #[test]
    fn select_all_with_nothing_to_select_asks_for_nothing() {
        let mut host = crate::host::UiHost::new(
            IntSize::new(10, 10),
            crate::unit::Density::new(1.0),
            Box::new(crate::fixed_text::FixedTextMeasurer::new()),
        );
        let handles = Rc::new(ContainerHandles::new());
        let given = handles.clone();
        host.set_content(move || {
            let modifier = Modifier.selection_container_with_handles(given.clone()).focusable();
            crate::layout::Layout(
                modifier,
                crate::layout::measure_policy(|scope, _, _| scope.layout(10, 10)),
                compose_runtime::callback(|| {}),
            );
        });
        host.frame();
        assert!(handles.container.get().is_some(), "attached");
        assert!(handles.run_menu_action(ContainerMenuAction::SelectAll));
        assert!(!handles.show_toolbar.get(), "no toolbar asked for");
        let container = handles.container.get().expect("attached");
        let focused = host
            .with_tree(|tree| tree.focused().is_some_and(|it| tree.is_at_or_below(it, container)));
        assert!(!focused, "no focus taken");
    }
}
