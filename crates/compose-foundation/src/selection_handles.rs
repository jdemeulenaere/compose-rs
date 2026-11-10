//! Touch selection's handles, as Compose's `SelectionHandle` and `CursorHandle` on Android: a
//! popup each (`HandlePopup`), at least 40 dp square to touch, drawn in
//! `LOCAL_TEXT_SELECTION_COLORS`' handle color, saying in its semantics which handle it is and
//! where it points (`SelectionHandleInfo`), and dragged by a finger
//! (docs/parity/design/touch-text-selection.md).
//!
//! A field's node decides which show and where; it tells them through [`TouchHandles`], which the
//! field's composable remembers and gives both to the node and to [`TextFieldHandles`], composed
//! inside the inner field.

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::snapshots::Snapshot as RuntimeSnapshot;
use compose_runtime::{OwnedState, composable};
use compose_ui::{
    Color, Dp, DrawModifierNode, DrawScope, EventContext, IntOffset, IntSize, Layout,
    LayoutDirection, Modifier, ModifierNode, ModifierNodeElement, NodeHandle, Offset, Paint,
    PointerEvent, PointerEventPass, PointerEventType, PointerInputModifierNode,
    PopupPositionProvider, PopupProperties, Rect, SemanticsConfiguration, SemanticsModifierNode,
    SemanticsPropertyKey, SemanticsValue, empty_measure_policy,
};

/// Which handle, as Compose's `Handle`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Handle {
    /// The cursor's, below a collapsed selection.
    Cursor,
    SelectionStart,
    SelectionEnd,
}

impl compose_runtime::ComposeArg for Handle {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl Handle {
    pub(crate) fn index(self) -> usize {
        match self {
            Handle::Cursor => 0,
            Handle::SelectionStart => 1,
            Handle::SelectionEnd => 2,
        }
    }
}

/// How a handle sits on the point it marks, as Compose's `SelectionHandleAnchor`: a selection's
/// start pointing left is anchored left (its top right on the point), the cursor's in the middle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectionHandleAnchor {
    Left,
    Middle,
    Right,
}

/// What a handle's node says of it, as Compose's `SelectionHandleInfo`: which handle, the point it
/// marks in the text's coordinates (the cursor's bottom center, a selection end's line bottom),
/// how it's anchored there, and whether its icon shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SelectionHandleInfo {
    pub handle: Handle,
    pub position: Offset,
    pub anchor: SelectionHandleAnchor,
    pub visible: bool,
}

impl SemanticsValue for SelectionHandleInfo {}

/// The key of a handle's [`SelectionHandleInfo`], as Compose's `SelectionHandleInfoKey`: what
/// tests find handles by (Compose's is internal, read by its tests; compose-rs's tests are
/// outside the crate).
pub static SELECTION_HANDLE_INFO_KEY: SemanticsPropertyKey<SelectionHandleInfo> =
    SemanticsPropertyKey::new("SelectionHandleInfo");

/// A handle's smallest size to touch, as Compose's `MinTouchTargetSizeForHandles` (Android's
/// `TextView`'s).
const MIN_TOUCH_TARGET: Dp = Dp(40.0);
/// A selection handle's icon, as Compose's `HandleWidth` and `HandleHeight`.
const HANDLE_SIZE: Dp = Dp(25.0);
/// The cursor handle's icon: as high as a selection handle, and as wide as that turned 45°
/// (Compose's `CursorHandleWidth`).
const CURSOR_HANDLE_WIDTH: Dp = Dp(25.0 * 2.0 / (1.0 + std::f32::consts::SQRT_2));

/// Where a selection handle at `offset` points in a text laid out as `layout` within `size`,
/// as Compose's `getSelectionHandleCoordinates`: at its line's bottom, and where the offset is
/// horizontally, on the side the run before or after it says (the start handle's own character
/// or the end handle's previous one, the other way when `crossed`), both kept within `size`.
/// `None` past the lines laid out (cut by the most lines).
pub(crate) fn selection_handle_coordinates(
    layout: &dyn compose_ui::Paragraph,
    text: &str,
    offset: usize,
    (start, crossed): (bool, bool),
    size: compose_ui::Size,
) -> Option<Offset> {
    let line = layout.get_line_for_offset(offset);
    if line >= layout.line_count() {
        return None;
    }
    let checked = if start != crossed { offset } else { previous_char(text, offset) };
    let primary = layout.get_bidi_run_direction(checked) == layout.get_paragraph_direction(offset);
    let x = layout.get_horizontal_position(offset, primary).clamp(0.0, size.width);
    let y = layout.get_line_bottom(line).clamp(0.0, size.height);
    Some(Offset::new(x, y))
}

/// The character boundary before `offset` in `text`, or 0.
pub(crate) fn previous_char(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].char_indices().next_back().map_or(0, |(index, _)| index)
}

/// Which handles show: the cursor's, and each selection end's with whether it points left
/// (Compose's `isLeftSelectionHandle`).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(crate) struct ShownHandles {
    pub cursor: bool,
    pub start: Option<bool>,
    pub end: Option<bool>,
}

impl ShownHandles {
    pub(crate) fn any(&self) -> bool {
        self.cursor || self.start.is_some() || self.end.is_some()
    }
}

/// Where each handle points (by [`Handle::index`]): in the text's coordinates, what its
/// semantics say, and in its anchor's (the inner field, its scroll taken off), where its popup
/// goes.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub(crate) struct HandlePositions {
    pub text: [Offset; 3],
    pub anchor: [Offset; 3],
}

/// What a field's node and its handles share: which show and where (states, so composition and
/// the handles' semantics follow them), the drag under way, and the field to tell of it.
pub(crate) struct TouchHandles {
    pub shown: OwnedState<ShownHandles>,
    pub positions: OwnedState<HandlePositions>,
    /// A handle's drag the field hasn't followed yet: the handle, and the finger's movement in the
    /// window since the drag passed the touch slop (Compose's `dragTotalDistance`, the slop taken
    /// off).
    /// Each move, in order: a selection handle's adjustment depends on the one before
    /// (`CharacterWithWordAccelerate`).
    pub moved: std::cell::RefCell<Vec<(Handle, Offset)>>,
    /// The drag ended (lifted or cancelled): the field forgets it, after its last move.
    pub ended: Cell<bool>,
    /// The cursor handle's taps the field hasn't followed yet: each toggles its text toolbar
    /// (D2032).
    pub cursor_taps: Cell<u32>,
    /// The inner field's position in the root after its last placement, while handles may show:
    /// where its toolbar goes from (D2032).
    pub origin: Cell<Offset>,
    /// What the field's text toolbar reads, made when it first shows (D2032).
    pub toolbar: std::cell::OnceCell<crate::text_field::FieldToolbar>,
    /// The field's node, measured again when a handle is dragged: the one focused last (a
    /// decoration moving the inner field makes the node anew).
    pub field: Cell<Option<NodeHandle>>,
    /// While handles may show: the inner field's bounds clipped by its ancestors are followed,
    /// as Compose's `textLayoutCoordinates.visibleBounds()`.
    pub active: OwnedState<bool>,
    /// The inner field's visible bounds, in its coordinates, after its last placement: a handle's
    /// point outside them hides it (one frame behind, as Compose's).
    pub visible: Cell<Option<Rect>>,
    /// A mouse's (or a touchpad's) event reached a handle: the touch interaction ends, as
    /// Compose's handles' `detectDirectTouchInteraction` (D2027).
    pub mouse: Cell<bool>,
    /// The field's default dropdown, made at its first right click (D2040).
    pub dropdown: OwnedState<Option<crate::text_context_menu::dropdown::DropdownSlot>>,
    /// What the field's magnifier follows, made where the platform has one (D2073).
    pub magnifier: std::cell::OnceCell<FieldMagnifier>,
    /// A selection handle pressed, its press not yet followed by the field: the magnifier shows
    /// from the press, as Compose's handles' `onDown` (`updateHandleDragging`).
    pub pressed: Cell<Option<Handle>>,
    /// A selection handle's press ended before the touch slop (tapped or cancelled): the
    /// magnifier goes, as Compose's `onUp` (`clearHandleDragging`).
    pub press_released: Cell<bool>,
}

/// What a field's magnifier follows (D2073).
pub(crate) struct FieldMagnifier {
    /// Where it's centered, in the inner field's coordinates, as Compose's
    /// `calculateSelectionMagnifierCenterAndroid`: unspecified while hidden (no handle's or long
    /// press's drag under way).
    pub center: OwnedState<Offset>,
    /// Its size once made (zero before).
    pub size: Cell<IntSize>,
}

impl TouchHandles {
    pub(crate) fn new() -> TouchHandles {
        TouchHandles {
            shown: compose_runtime::mutable_state_of(ShownHandles::default()),
            positions: compose_runtime::mutable_state_of(HandlePositions::default()),
            moved: std::cell::RefCell::new(Vec::new()),
            ended: Cell::new(false),
            cursor_taps: Cell::new(0),
            origin: Cell::new(Offset::ZERO),
            toolbar: std::cell::OnceCell::new(),
            field: Cell::new(None),
            active: compose_runtime::mutable_state_of(false),
            visible: Cell::new(None),
            mouse: Cell::new(false),
            dropdown: compose_runtime::mutable_state_of(None),
            magnifier: std::cell::OnceCell::new(),
            pressed: Cell::new(None),
            press_released: Cell::new(false),
        }
    }

    /// The platform has a magnifier: the field's drags show it (D2073).
    pub(crate) fn enable_magnifier(&self) {
        self.magnifier.get_or_init(|| FieldMagnifier {
            center: compose_runtime::mutable_state_of(Offset::UNSPECIFIED),
            size: Cell::new(IntSize::ZERO),
        });
    }

    /// Hides the magnifier now, outside a measurement (a gesture cancelled, the field gone).
    pub(crate) fn hide_magnifier(&self) {
        let Some(magnifier) = self.magnifier.get() else { return };
        let center = RuntimeSnapshot::without_read_observation(|| magnifier.center.get());
        if center.is_specified() {
            magnifier.center.set(Offset::UNSPECIFIED);
        }
    }

    /// The field's node `field` is gone: nothing it told shows any more, and drags aren't told
    /// to it.
    pub(crate) fn forget(&self, field: Option<NodeHandle>) {
        if field.is_some() && self.field.get() == field {
            self.field.set(None);
        }
        if self.shown.with(ShownHandles::any) {
            self.shown.set(ShownHandles::default());
        }
        if self.active.with(|active| *active) {
            self.active.set(false);
        }
        self.hide_magnifier();
    }

    fn measure_field(&self) {
        if let Some(field) = self.field.get() {
            field.invalidate_measurement();
        }
    }

    fn tell_field(&self, moved: Option<(Handle, Offset)>) {
        match moved {
            Some(moved) => self.moved.borrow_mut().push(moved),
            None => self.ended.set(true),
        }
        self.measure_field();
    }
}

impl compose_runtime::ComposeArg for TouchHandles {
    fn same(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// The handles of a field, composed inside its inner field, as Compose's `TextFieldCursorHandle`
/// and `TextFieldSelectionHandles`: each only while its node says it shows.
#[composable]
#[allow(non_snake_case)]
pub(crate) fn TextFieldHandles(handles: Rc<TouchHandles>) {
    crate::text_context_menu::dropdown::DefaultDropdownMenu(handles.dropdown.get());
    if handles.active.get() {
        VisibleBoundsReader(handles.clone());
    }
    let shown = handles.shown.get();
    if !shown.any() {
        return;
    }
    let source: Rc<dyn HandleSource> = handles;
    if shown.cursor {
        HandlePopup(source.clone(), Handle::Cursor, false, Modifier, true);
    }
    if let Some(left) = shown.start {
        HandlePopup(source.clone(), Handle::SelectionStart, left, Modifier, true);
    }
    if let Some(left) = shown.end {
        HandlePopup(source, Handle::SelectionEnd, left, Modifier, true);
    }
}

/// What a handle's popup asks of what shows it: a field's [`TouchHandles`], a container's
/// handles, or a test's offset provider (Compose's `OffsetProvider` with the drag observer).
pub(crate) trait HandleSource {
    /// Where `handle` points in its popup's anchor's coordinates, `None` while it's hidden
    /// (`Offset.Unspecified`).
    fn anchor_point(&self, handle: Handle) -> Option<Offset>;
    /// Where its semantics say it points (`SelectionHandleInfo.position`).
    fn info_point(&self, handle: Handle) -> Option<Offset>;
    /// Its drag: the finger's movement since it passed the touch slop.
    fn drag(&self, handle: Handle, moved: Offset, context: &mut EventContext<'_>);
    /// Its drag ended (lifted, cancelled, or its node gone).
    fn drag_ended(&self, handle: Handle);
    /// It was tapped (a press lifted before the touch slop).
    fn tapped(&self, _handle: Handle) {}
    /// Its press was cancelled before the touch slop (no tap, no drag).
    fn press_cancelled(&self, _handle: Handle) {}
    /// Its node left while pressed, before the touch slop: a field's press ends, as Compose's
    /// press gesture's `invokeOnCompletion { clearHandleDragging() }`; a container's doesn't
    /// (Compose's `detectPreDragGesturesWithObserver` calls no `onUp` when cancelled).
    fn press_detached(&self, _handle: Handle) {}
    /// It was pressed.
    fn pressed(&self, _handle: Handle) {}
    /// A mouse's (or a touchpad's) event reached it.
    fn mouse_event(&self) {}
}

impl crate::selection_magnifier::SelectionMagnifierSource for TouchHandles {
    fn magnifier_center(&self) -> Offset {
        self.magnifier.get().map_or(Offset::UNSPECIFIED, |magnifier| magnifier.center.get())
    }
    fn set_magnifier_size(&self, size: IntSize) {
        if let Some(magnifier) = self.magnifier.get()
            && magnifier.size.replace(size) != size
        {
            self.measure_field();
        }
    }
}

impl HandleSource for TouchHandles {
    fn anchor_point(&self, handle: Handle) -> Option<Offset> {
        Some(self.positions.with(|positions| positions.anchor[handle.index()]))
    }
    fn info_point(&self, handle: Handle) -> Option<Offset> {
        Some(self.positions.with(|positions| positions.text[handle.index()]))
    }
    fn drag(&self, handle: Handle, moved: Offset, _context: &mut EventContext<'_>) {
        self.tell_field(Some((handle, moved)));
    }
    fn drag_ended(&self, _handle: Handle) {
        self.tell_field(None);
    }
    fn tapped(&self, handle: Handle) {
        if handle == Handle::Cursor {
            self.cursor_taps.set(self.cursor_taps.get() + 1);
            self.measure_field();
        } else {
            self.press_cancelled(handle);
        }
    }
    /// A selection handle's press shows the magnifier (D2073).
    fn pressed(&self, handle: Handle) {
        if handle != Handle::Cursor && self.magnifier.get().is_some() {
            self.pressed.set(Some(handle));
            self.measure_field();
        }
    }
    fn press_cancelled(&self, handle: Handle) {
        if handle != Handle::Cursor && self.magnifier.get().is_some() {
            self.pressed.set(None);
            self.press_released.set(true);
            self.measure_field();
        }
    }
    /// The magnifier its press showed hides (D2106).
    fn press_detached(&self, handle: Handle) {
        self.press_cancelled(handle);
    }
    fn mouse_event(&self) {
        if !self.mouse.replace(true) {
            self.measure_field();
        }
    }
}

/// A `SelectionContainer`'s handles: the tree holding its selection says where they point, in
/// the container's coordinates, and takes their drags.
struct ContainerSource(Rc<compose_ui::ContainerHandles>);

impl HandleSource for ContainerSource {
    fn anchor_point(&self, handle: Handle) -> Option<Offset> {
        self.0.position(handle == Handle::SelectionStart)
    }
    fn info_point(&self, handle: Handle) -> Option<Offset> {
        self.anchor_point(handle)
    }
    fn drag(&self, handle: Handle, moved: Offset, context: &mut EventContext<'_>) {
        self.0.drag(context, handle == Handle::SelectionStart, moved);
    }
    fn drag_ended(&self, _handle: Handle) {
        self.0.end_drag();
    }
    /// Its toolbar shows again, as Compose's handle observer's `onUp` (D2041).
    fn tapped(&self, _handle: Handle) {
        self.0.handle_tapped();
    }
    /// Its toolbar shows again, as Compose's handle observer's `onUp`: a cancelled press ends
    /// with an up there, and `done()` shows the toolbar.
    fn press_cancelled(&self, _handle: Handle) {
        self.0.handle_press_cancelled();
    }
    /// Its toolbar hides while it's held, as Compose's handle observer's `onDown` (D2041).
    fn pressed(&self, handle: Handle) {
        self.0.handle_pressed(handle == Handle::SelectionStart);
    }
}

/// The selection handles of a `SelectionContainer`, composed after its content, as Compose's
/// `SelectionContainer`: both, while it's in touch mode and focused over a selection not
/// trivially collapsed; each hidden (its icon not drawn, its semantics not visible) while it
/// points outside the container's visible bounds.
#[composable]
#[allow(non_snake_case)]
pub(crate) fn SelectionContainerHandles(handles: Rc<compose_ui::ContainerHandles>) {
    let Some(shown) = handles.shown() else { return };
    let source: Rc<dyn HandleSource> =
        compose_runtime::remember_keyed(Rc::as_ptr(&handles) as usize, || {
            Rc::new(ContainerSource(handles.clone())) as Rc<dyn HandleSource>
        });
    for (handle, direction) in [
        (Handle::SelectionStart, shown.start_direction),
        (Handle::SelectionEnd, shown.end_direction),
    ] {
        let start = handle == Handle::SelectionStart;
        let left = is_left_selection_handle(start, direction, shown.crossed);
        compose_runtime::key(start, || {
            HandlePopup(source.clone(), handle, left, Modifier, false);
        });
    }
}

/// Whether a selection handle points left, as Compose's `isLeftSelectionHandle`: the start's
/// where the text runs left to right, the end's where it runs right to left, the other way when
/// the handles are crossed.
#[doc(hidden)]
pub fn is_left_selection_handle(
    start: bool,
    direction: compose_ui::ResolvedTextDirection,
    crossed: bool,
) -> bool {
    if start {
        is_handle_ltr_direction(direction, crossed)
    } else {
        !is_handle_ltr_direction(direction, crossed)
    }
}

/// Whether a handle's text runs left to right once crossed handles are counted, as Compose's
/// `isHandleLtrDirection`.
#[doc(hidden)]
pub fn is_handle_ltr_direction(
    direction: compose_ui::ResolvedTextDirection,
    crossed: bool,
) -> bool {
    (direction == compose_ui::ResolvedTextDirection::Ltr) != crossed
}

/// A test's handle: points where `offset_provider` says (`None`: hidden), dragged nowhere.
struct ProvidedSource(Rc<dyn Fn() -> Option<Offset>>);

impl HandleSource for ProvidedSource {
    fn anchor_point(&self, _handle: Handle) -> Option<Offset> {
        (self.0)()
    }
    fn info_point(&self, _handle: Handle) -> Option<Offset> {
        (self.0)()
    }
    fn drag(&self, _handle: Handle, _moved: Offset, _context: &mut EventContext<'_>) {}
    fn drag_ended(&self, _handle: Handle) {}
}

/// A selection handle in a popup, as Compose's internal `SelectionHandle` (Android's): pointing
/// where `offset_provider` says, in the coordinates of the layout around it (`None`: hidden, its
/// icon not drawn and its semantics not visible), left or right by the direction at it and
/// whether the handles are crossed, its node at least 40 dp square and `modifier`'s. What
/// Compose's tests of handles compose; a container's and a field's are composed alike.
#[doc(hidden)]
#[composable]
#[allow(non_snake_case)]
pub fn SelectionHandle(
    offset_provider: Rc<dyn Fn() -> Option<Offset>>,
    is_start_handle: bool,
    direction: compose_ui::ResolvedTextDirection,
    handles_crossed: bool,
    #[default(crate::Modifier)] modifier: Modifier,
) {
    let left = is_left_selection_handle(is_start_handle, direction, handles_crossed);
    let source: Rc<dyn HandleSource> =
        compose_runtime::remember_keyed(Rc::as_ptr(&offset_provider) as *const () as usize, || {
            Rc::new(ProvidedSource(offset_provider.clone())) as Rc<dyn HandleSource>
        });
    let handle = if is_start_handle { Handle::SelectionStart } else { Handle::SelectionEnd };
    HandlePopup(source, handle, left, modifier, false);
}

/// A selection handle's icon alone, as Compose's `SelectionHandleIcon`: 25 dp square, pointing
/// left or right, drawn while `icon_visible` says so.
#[doc(hidden)]
#[composable]
#[allow(non_snake_case)]
pub fn SelectionHandleIcon(modifier: Modifier, icon_visible: Rc<dyn Fn() -> bool>, is_left: bool) {
    let element = IconElement { visible: icon_visible, left: is_left };
    Layout(
        modifier.size((HANDLE_SIZE, HANDLE_SIZE)).then_custom(element),
        empty_measure_policy(),
        compose_runtime::callback(|| {}),
    );
}

#[derive(Clone)]
struct IconElement {
    visible: Rc<dyn Fn() -> bool>,
    left: bool,
}

struct IconNode(IconElement);

impl ModifierNodeElement for IconElement {
    type Node = IconNode;
    fn create(&self) -> IconNode {
        IconNode(self.clone())
    }
    fn update(&self, node: &mut IconNode) {
        node.0 = self.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.visible, &other.visible) && self.left == other.left
    }
    fn name(&self) -> &'static str {
        "selection_handle_icon"
    }
}

impl ModifierNode for IconNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for IconNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        if (self.0.visible)() {
            draw_selection_icon(scope, self.0.left);
        }
    }
}

/// A node of no size in the inner field that follows the field's bounds clipped by its
/// ancestors after each layout, for the field's handles, while they may show.
#[composable]
#[allow(non_snake_case)]
fn VisibleBoundsReader(handles: Rc<TouchHandles>) {
    Layout(
        Modifier.then_custom(VisibleBoundsElement(handles)),
        empty_measure_policy(),
        compose_runtime::callback(|| {}),
    );
}

#[derive(Clone)]
struct VisibleBoundsElement(Rc<TouchHandles>);

struct VisibleBoundsNode(Rc<TouchHandles>);

impl ModifierNodeElement for VisibleBoundsElement {
    type Node = VisibleBoundsNode;
    fn create(&self) -> VisibleBoundsNode {
        VisibleBoundsNode(self.0.clone())
    }
    fn update(&self, node: &mut VisibleBoundsNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    fn name(&self) -> &'static str {
        "selection_handles_visible_bounds"
    }
}

impl ModifierNode for VisibleBoundsNode {
    fn as_global_position_aware(
        &mut self,
    ) -> Option<&mut dyn compose_ui::GlobalPositionAwareModifierNode> {
        Some(self)
    }
    fn on_detach(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.0.visible.set(None);
    }
}

impl compose_ui::GlobalPositionAwareModifierNode for VisibleBoundsNode {
    /// The inner field's bounds in the window, clipped by its ancestors and the window, back in
    /// its coordinates, as Compose's `LayoutCoordinates.visibleBounds()`; the field measures
    /// again when they changed.
    fn on_globally_positioned(
        &mut self,
        coordinates: compose_ui::LayoutCoordinates,
        _context: &mut compose_ui::NodeContext<'_>,
    ) {
        let Some(field) = coordinates.parent_layout_coordinates() else { return };
        let window = field.bounds_in_window();
        let top_left = field.window_to_local(Offset::new(window.left, window.top));
        let bottom_right = field.window_to_local(Offset::new(window.right, window.bottom));
        let visible = Rect::new(top_left.x, top_left.y, bottom_right.x, bottom_right.y);
        // Where its text toolbar goes from, followed while it shows (D2032).
        let origin = field.local_to_root(Offset::ZERO);
        self.0.origin.set(origin);
        if let Some(toolbar) = self.0.toolbar.get()
            && toolbar.origin.with(|it| *it != origin)
        {
            toolbar.origin.set(origin);
        }
        if self.0.visible.get() != Some(visible) {
            self.0.visible.set(Some(visible));
            self.0.measure_field();
        }
    }
}

/// One handle's popup, as Compose's `HandlePopup` with its content: not focusable, never
/// dismissed, its node 40 dp square with a `touch_target` (a field's, Compose's
/// `MinTouchTargetSizeForHandles`), else its icon's 25 dp (a container's).
#[composable]
#[allow(non_snake_case)]
fn HandlePopup(
    source: Rc<dyn HandleSource>,
    handle: Handle,
    left: bool,
    modifier: Modifier,
    touch_target: bool,
) {
    let key = (handle, left, Rc::as_ptr(&source) as *const () as usize);
    let provider = compose_runtime::remember_keyed(key, || {
        Rc::new(HandlePositionProvider {
            source: source.clone(),
            handle,
            left,
            previous: Cell::new(Offset::ZERO),
        }) as Rc<dyn PopupPositionProvider>
    });
    let element = HandleElement { source: source.clone(), handle, left };
    compose_ui::PopupWithPositionProvider(
        provider,
        None,
        PopupProperties::default(),
        compose_runtime::callback(move || {
            let edge = if touch_target { MIN_TOUCH_TARGET } else { HANDLE_SIZE };
            let size = (edge, edge);
            Layout(
                modifier.clone().required_size(size).then_custom(element.clone()),
                empty_measure_policy(),
                compose_runtime::callback(|| {}),
            );
        }),
    );
}

/// Places a handle's popup on the point it marks, as Compose's `HandlePositionProvider`: a left
/// handle's top right there, a right handle's top left, the cursor handle's top center.
struct HandlePositionProvider {
    source: Rc<dyn HandleSource>,
    handle: Handle,
    left: bool,
    /// Where it last pointed: a hidden handle stays there, as Compose's `prevPosition`.
    previous: Cell<Offset>,
}

impl PopupPositionProvider for HandlePositionProvider {
    fn position(
        &self,
        anchor: Rect,
        _window: IntSize,
        _direction: LayoutDirection,
        popup: IntSize,
    ) -> IntOffset {
        let point =
            RuntimeSnapshot::without_read_observation(|| self.source.anchor_point(self.handle))
                .unwrap_or(self.previous.get());
        self.previous.set(point);
        let adjustment = match (self.handle, self.left) {
            (Handle::Cursor, _) => -(popup.width / 2),
            (_, true) => -popup.width,
            (_, false) => 0,
        };
        IntOffset::new(
            anchor.left.round() as i32 + point.x.round() as i32 + adjustment,
            anchor.top.round() as i32 + point.y.round() as i32,
        )
    }
}

#[derive(Clone)]
struct HandleElement {
    source: Rc<dyn HandleSource>,
    handle: Handle,
    left: bool,
}

/// A handle's node: its semantics, its icon, and its drag.
struct HandleNode {
    source: Rc<dyn HandleSource>,
    handle: Handle,
    left: bool,
    /// Where the finger went down, in the window, while it's held.
    down: Option<Offset>,
    /// Where the drag counts from once past the touch slop: the press, moved by the slop toward
    /// the finger, as Compose's `awaitTouchSlopOrCancellation` leaves the drag's first delta.
    origin: Option<Offset>,
}

impl ModifierNodeElement for HandleElement {
    type Node = HandleNode;
    fn create(&self) -> HandleNode {
        HandleNode {
            source: self.source.clone(),
            handle: self.handle,
            left: self.left,
            down: None,
            origin: None,
        }
    }
    fn update(&self, node: &mut HandleNode) {
        node.source = self.source.clone();
        node.handle = self.handle;
        node.left = self.left;
    }
    fn same(&self, other: &Self) -> bool {
        std::ptr::addr_eq(Rc::as_ptr(&self.source), Rc::as_ptr(&other.source))
            && self.handle == other.handle
            && self.left == other.left
    }
    fn name(&self) -> &'static str {
        "selection_handle"
    }
}

impl ModifierNode for HandleNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
    fn observes_reads(&self) -> bool {
        true
    }
    fn on_detach(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        if self.down.is_some() && self.origin.is_none() {
            self.source.press_detached(self.handle);
        }
        self.end_drag();
    }
}

impl HandleNode {
    fn end_drag(&mut self) {
        self.down = None;
        if self.origin.take().is_some() {
            self.source.drag_ended(self.handle);
        }
    }
}

impl SemanticsModifierNode for HandleNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        let position = self.source.info_point(self.handle);
        let anchor = match (self.handle, self.left) {
            (Handle::Cursor, _) => SelectionHandleAnchor::Middle,
            (_, true) => SelectionHandleAnchor::Left,
            (_, false) => SelectionHandleAnchor::Right,
        };
        config.set(
            &SELECTION_HANDLE_INFO_KEY,
            SelectionHandleInfo {
                handle: self.handle,
                position: position.unwrap_or(Offset::UNSPECIFIED),
                anchor,
                visible: position.is_some(),
            },
        );
    }
}

impl DrawModifierNode for HandleNode {
    /// The handle's shape, as Compose's `createHandleImage`: a circle with the square of its top
    /// left corner filled, so it points up and left from its top left; flipped for a left handle,
    /// turned 45° for the cursor's, at the side or the center of its node where the point is.
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        if self.source.anchor_point(self.handle).is_none() {
            return;
        }
        let color: Color = scope.current_value_of(&crate::LOCAL_TEXT_SELECTION_COLORS).handle_color;
        let density = scope.density();
        let size = scope.size();
        match self.handle {
            Handle::Cursor => {
                let width = density.to_px(CURSOR_HANDLE_WIDTH);
                let radius = width / 2.0;
                let left = (size.width - width) / 2.0;
                scope.translate(left + radius, 0.0, |scope| {
                    scope.rotate(45.0, Offset::ZERO, |scope| draw_shape(scope, radius, color));
                });
            }
            _ => {
                let edge = density.to_px(HANDLE_SIZE);
                // Its top right on the point: at the node's right.
                let left = if self.left { size.width - edge } else { 0.0 };
                scope.translate(left, 0.0, |scope| draw_icon(scope, edge, self.left, color));
            }
        }
    }
}

/// A selection handle's icon in a scope its size, in `LOCAL_TEXT_SELECTION_COLORS`' handle color.
fn draw_selection_icon(scope: &mut DrawScope<'_>, left: bool) {
    let color: Color = scope.current_value_of(&crate::LOCAL_TEXT_SELECTION_COLORS).handle_color;
    let edge = scope.size().width;
    draw_icon(scope, edge, left, color);
}

/// A selection handle's icon `edge` square at the scope's top left: pointing up and left, or
/// mirrored to point up and right (`left`, the left handle's).
fn draw_icon(scope: &mut DrawScope<'_>, edge: f32, left: bool, color: Color) {
    let radius = edge / 2.0;
    if left {
        scope.scale(-1.0, 1.0, Offset::new(radius, radius), |scope| {
            draw_shape(scope, radius, color);
        });
    } else {
        draw_shape(scope, radius, color);
    }
}

/// A circle of `radius` at (radius, radius) and the square of its top left quarter.
fn draw_shape(scope: &mut DrawScope<'_>, radius: f32, color: Color) {
    scope.draw_rect(Rect::new(0.0, 0.0, radius, radius), Paint::fill(color));
    scope.draw_circle(Offset::new(radius, radius), radius, Paint::fill(color));
}

impl PointerInputModifierNode for HandleNode {
    /// A press, then moves past the touch slop: a drag, each move told the field; a press lifted
    /// before the slop is a tap (the cursor handle's toggles the toolbar).
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass != PointerEventPass::Main {
            return;
        }
        if event.pointer_type == compose_ui::PointerType::Mouse {
            self.source.mouse_event();
        }
        let Some(position) = context.pointer_position() else { return };
        match event.kind {
            PointerEventType::Press if !event.consumed => {
                event.consume();
                self.end_drag();
                self.down = Some(position);
                self.source.pressed(self.handle);
            }
            PointerEventType::Move if event.pressed => {
                let Some(down) = self.down else { return };
                if self.origin.is_none() {
                    let moved = position - down;
                    let distance = moved.x.hypot(moved.y);
                    let slop = context.view_configuration().touch_slop;
                    if distance < slop {
                        return;
                    }
                    let toward = Offset::new(moved.x / distance * slop, moved.y / distance * slop);
                    self.origin = Some(down + toward);
                }
                event.consume();
                let origin = self.origin.expect("past the slop");
                self.source.drag(self.handle, position - origin, context);
            }
            PointerEventType::Release => {
                if self.down.is_some() && self.origin.is_none() {
                    self.source.tapped(self.handle);
                }
                if self.down.is_some() {
                    event.consume();
                }
                self.end_drag();
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        // A press cancelled before the slop is neither a tap nor a drag's end.
        if self.down.is_some() && self.origin.is_none() {
            self.source.press_cancelled(self.handle);
        }
        self.end_drag();
    }
}
