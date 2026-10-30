//! `ExposedDropdownMenuBox` and `ExposedDropdownMenu`, as Compose's Material 3: a menu at a text
//! field (or another anchor) that opens it, and takes the focus unless the anchor is typed in.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::MutableTransitionState;
use compose_foundation::BorderStroke;
use compose_foundation::layout::{Box, ColumnScope, WindowInsets};
use compose_runtime::{Callback, ComposeArg, OwnedState, composable, mutable_state_of, remember};
use compose_ui::{
    Alignment, Color, Constraints, Density, Dp, EventContext, FocusRequester, IntOffset, IntSize,
    Key, KeyEvent, KeyEventType, KeyInputModifierNode, LayoutDirection, LayoutModifierNode,
    Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement,
    PaddingValues, PointerEvent, PointerEventPass, PointerEventType, PointerInputModifierNode,
    PopupPositionProvider, PopupProperties, Rect, Role, SemanticsConfiguration,
    SemanticsModifierNode, Shape, SoftwareKeyboardController, TransformOrigin,
};

use crate::dialog::{MENU_VERTICAL_MARGIN, dropdown_menu_content, transform_origin};
use crate::theme::MaterialTheme;
use crate::{TextFieldColors, TextFieldDefaults};

/// The kind of anchor of a menu, as Compose's `ExposedDropdownMenuAnchorType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExposedDropdownMenuAnchorType {
    /// A read-only text field: opening the menu moves the focus to it.
    PrimaryNotEditable,
    /// A text field typed in: the focus stays there as the menu opens.
    PrimaryEditable,
    /// Something beside an editable anchor, such as the icon in its text field: as
    /// `PrimaryEditable`, the focus stays in the text field.
    SecondaryEditable,
}

impl ComposeArg for ExposedDropdownMenuAnchorType {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ExposedDropdownMenuAnchorType {
    fn has_greater_or_equal_priority_than(self, that: Self) -> bool {
        self != Self::SecondaryEditable || that == Self::SecondaryEditable
    }
}

/// What the box, its anchors and its menu share, kept for the box's life, so an anchor's nodes
/// stay the same as the box recomposes.
struct Shared {
    /// The box's bounds in the window, written as it's placed and read by what measures the
    /// menu, so that a scroll recomposes nothing.
    anchor_bounds: OwnedState<Rect>,
    anchor_type: OwnedState<ExposedDropdownMenuAnchorType>,
    always_focusable: OwnedState<bool>,
    focus_requester: FocusRequester,
    ime: WindowInsets,
    // The latest of these, as the box last composed.
    on_expanded_change: RefCell<Callback<dyn Fn(bool)>>,
    keyboard: RefCell<Option<SoftwareKeyboardController>>,
}

impl Shared {
    /// What an anchor's click does: the anchor that was clicked is the menu's anchor now.
    fn toggle(&self, anchor_type: ExposedDropdownMenuAnchorType, expanded: bool) {
        self.anchor_type.set(anchor_type);
        let on_expanded_change = self.on_expanded_change.borrow().clone();
        on_expanded_change(!expanded);
    }
}

/// What `ExposedDropdownMenuBox`'s content is given, as `ExposedDropdownMenuBoxScope`: the
/// anchors take it in `menu_anchor`, the menu as its first argument.
#[derive(Clone)]
pub struct ExposedDropdownMenuBoxScope {
    shared: Rc<Shared>,
    expanded: bool,
}

impl ComposeArg for ExposedDropdownMenuBoxScope {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.shared, &other.shared) && self.expanded == other.expanded
    }
}

impl ExposedDropdownMenuBoxScope {
    /// The anchor that last opened the menu or attached, as the internal `anchorType`.
    #[doc(hidden)]
    pub fn anchor_type(&self) -> ExposedDropdownMenuAnchorType {
        self.shared.anchor_type.get()
    }
}

/// A box for a menu and what opens it, as Compose's `ExposedDropdownMenuBox`: `content` has a
/// text field with `menu_anchor` and an `ExposedDropdownMenu`. It says when the anchor's clicked
/// with `on_expanded_change`, and also on back while `expanded`.
#[composable]
#[allow(non_snake_case)]
pub fn ExposedDropdownMenuBox(
    expanded: bool,
    on_expanded_change: impl Fn(bool) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    content: impl Fn(ExposedDropdownMenuBoxScope) + 'static,
) {
    let shared = remember(|| {
        Rc::new(Shared {
            anchor_bounds: mutable_state_of(Rect::new(0.0, 0.0, 0.0, 0.0)),
            anchor_type: mutable_state_of(ExposedDropdownMenuAnchorType::PrimaryNotEditable),
            always_focusable: mutable_state_of(false),
            focus_requester: FocusRequester::new(),
            ime: WindowInsets::ime(),
            on_expanded_change: RefCell::new(on_expanded_change.clone()),
            keyboard: RefCell::new(None),
        })
    });
    shared.on_expanded_change.replace(on_expanded_change.clone());
    shared.keyboard.replace(compose_ui::LOCAL_SOFTWARE_KEYBOARD_CONTROLLER.current());

    let positioned = shared.clone();
    let scope = ExposedDropdownMenuBoxScope { shared: shared.clone(), expanded };
    Box(
        modifier.on_globally_positioned(move |coordinates: compose_ui::LayoutCoordinates| {
            let (position, size) = (coordinates.position_in_window(), coordinates.size());
            positioned.anchor_bounds.set(Rect::new(
                position.x,
                position.y,
                position.x + size.width as f32,
                position.y + size.height as f32,
            ));
        }),
        Alignment::TOP_START,
        false,
        compose_runtime::callback(move |_| content(scope.clone())),
    );

    compose_runtime::SideEffect({
        let focus_requester = shared.focus_requester;
        move || {
            if expanded {
                focus_requester.request_focus();
            }
        }
    });

    // Back goes to the popup first if the menu has the focus; if not, it's handled here.
    compose_ui::BackHandler(expanded, compose_runtime::callback(move || on_expanded_change(false)));
}

/// The modifiers of what's in an `ExposedDropdownMenuBox`, as `ExposedDropdownMenuBoxScope`'s.
pub trait ExposedDropdownMenuBoxModifiers {
    /// What opens the menu, typically the text field or the icon in it, as `Modifier.menuAnchor`:
    /// takes the focus when the menu opens, toggles the menu when clicked.
    fn menu_anchor(
        self,
        scope: &ExposedDropdownMenuBoxScope,
        anchor_type: ExposedDropdownMenuAnchorType,
    ) -> Modifier;
    /// As `menu_anchor`; not `enabled`, it opens and closes nothing and says nothing to
    /// accessibility, as `menuAnchor(type, enabled)`.
    fn menu_anchor_with(
        self,
        scope: &ExposedDropdownMenuBoxScope,
        anchor_type: ExposedDropdownMenuAnchorType,
        enabled: bool,
    ) -> Modifier;
    /// A menu's size at its anchor, as `Modifier.exposedDropdownSize`: no taller than the larger
    /// space above or below the anchor, and as wide as it if `match_anchor_width`. For a custom
    /// menu: `ExposedDropdownMenu` has it already.
    fn exposed_dropdown_size(
        self,
        scope: &ExposedDropdownMenuBoxScope,
        match_anchor_width: bool,
    ) -> Modifier;
}

impl ExposedDropdownMenuBoxModifiers for Modifier {
    fn menu_anchor(
        self,
        scope: &ExposedDropdownMenuBoxScope,
        anchor_type: ExposedDropdownMenuAnchorType,
    ) -> Modifier {
        self.menu_anchor_with(scope, anchor_type, true)
    }

    fn menu_anchor_with(
        self,
        scope: &ExposedDropdownMenuBoxScope,
        anchor_type: ExposedDropdownMenuAnchorType,
        enabled: bool,
    ) -> Modifier {
        let shared = &scope.shared;
        let anchored = self
            .focus_requester(shared.focus_requester)
            .then_custom(AnchorElement { shared: shared.clone(), anchor_type });
        if !enabled {
            return anchored;
        }
        anchored.then_custom(ExpandableElement {
            shared: shared.clone(),
            anchor_type,
            expanded: scope.expanded,
        })
    }

    fn exposed_dropdown_size(
        self,
        scope: &ExposedDropdownMenuBoxScope,
        match_anchor_width: bool,
    ) -> Modifier {
        self.then_custom(SizeElement { shared: scope.shared.clone(), match_anchor_width })
    }
}

/// Makes the type the box's when it attaches, unless a primary anchor has that, as
/// `ExposedDropdownMenuAnchorElement`: the menu knows before the first click.
struct AnchorElement {
    shared: Rc<Shared>,
    anchor_type: ExposedDropdownMenuAnchorType,
}

struct AnchorNode {
    shared: Rc<Shared>,
    anchor_type: ExposedDropdownMenuAnchorType,
}

impl ModifierNodeElement for AnchorElement {
    type Node = AnchorNode;
    fn create(&self) -> AnchorNode {
        AnchorNode { shared: self.shared.clone(), anchor_type: self.anchor_type }
    }
    fn update(&self, node: &mut AnchorNode) {
        node.shared = self.shared.clone();
        node.anchor_type = self.anchor_type;
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.shared, &other.shared) && self.anchor_type == other.anchor_type
    }
    fn name(&self) -> &'static str {
        "exposedDropdownMenuAnchorType"
    }
}

impl ModifierNode for AnchorNode {
    fn on_attach(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        if self.anchor_type.has_greater_or_equal_priority_than(self.shared.anchor_type.get()) {
            self.shared.anchor_type.set(self.anchor_type);
        }
    }
}

/// An anchor's click, keys and semantics, as Compose's private `Modifier.expandable`: one node,
/// which an update reaches only when the menu opens or closes.
struct ExpandableElement {
    shared: Rc<Shared>,
    anchor_type: ExposedDropdownMenuAnchorType,
    expanded: bool,
}

struct ExpandableNode {
    shared: Rc<Shared>,
    anchor_type: ExposedDropdownMenuAnchorType,
    expanded: bool,
    /// Between a press and its release, as Compose's `awaitEachGesture`.
    pressed: bool,
}

impl ModifierNodeElement for ExpandableElement {
    type Node = ExpandableNode;
    fn create(&self) -> ExpandableNode {
        ExpandableNode {
            shared: self.shared.clone(),
            anchor_type: self.anchor_type,
            expanded: self.expanded,
            pressed: false,
        }
    }
    fn update(&self, node: &mut ExpandableNode) {
        node.shared = self.shared.clone();
        node.anchor_type = self.anchor_type;
        node.expanded = self.expanded;
        // A press the menu's opening or closing saw belongs to that: its release does nothing,
        // as Compose's `pointerInput` starting again. It's how a press that dismissed the menu
        // from outside doesn't open it again.
        node.pressed = false;
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.shared, &other.shared)
            && self.anchor_type == other.anchor_type
            && self.expanded == other.expanded
    }
    fn name(&self) -> &'static str {
        "expandable"
    }
}

impl ModifierNode for ExpandableNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for ExpandableNode {
    /// In the initial pass, not `clickable`, so the text field in it gets the focus first, not
    /// the box, as Compose's comment says.
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        match (pass, event.kind) {
            (PointerEventPass::Initial, PointerEventType::Press) if !event.consumed => {
                self.pressed = true;
                if self.anchor_type == ExposedDropdownMenuAnchorType::SecondaryEditable {
                    event.consume();
                }
            }
            (PointerEventPass::Initial, PointerEventType::Release) if self.pressed => {
                self.pressed = false;
                let (shared, anchor_type, expanded) =
                    (self.shared.clone(), self.anchor_type, self.expanded);
                context.run(move || shared.toggle(anchor_type, expanded));
            }
            // Dragged out of the anchor, or a scroll took it, as `waitForUpOrCancellation`.
            (PointerEventPass::Initial, PointerEventType::Move) => {
                let (x, y) = (event.position.x, event.position.y);
                let inside =
                    x >= 0.0 && y >= 0.0 && x < bounds.width as f32 && y < bounds.height as f32;
                self.pressed &= inside && !event.consumed;
            }
            (PointerEventPass::Final, PointerEventType::Move) => {
                self.pressed &= !event.consumed;
            }
            _ => {}
        }
    }
}

impl KeyInputModifierNode for ExpandableNode {
    fn on_pre_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        // As if `clickable` was set, as Compose's comment says.
        let enter = matches!(event.key, Key::DirectionCenter | Key::Enter | Key::NumPadEnter);
        let (shared, anchor_type, expanded) =
            (self.shared.clone(), self.anchor_type, self.expanded);
        let editable = anchor_type == ExposedDropdownMenuAnchorType::PrimaryEditable;
        if event.kind == KeyEventType::KeyUp && (enter || event.key == Key::Space) {
            if !editable {
                context.run(move || shared.toggle(anchor_type, expanded));
            } else if enter {
                // Space is typed: it's not for the menu.
                context.run(move || shared.toggle(anchor_type, expanded));
                return true;
            }
        }
        // The menu of what's typed in doesn't take the focus, so as not to interrupt: it does
        // when the keyboard moves to it.
        if editable
            && self.expanded
            && matches!(event.key, Key::Tab | Key::DirectionDown | Key::DirectionUp)
        {
            self.shared.always_focusable.set(true);
            return true;
        }
        self.shared.always_focusable.set(false);
        false
    }
}

impl SemanticsModifierNode for ExpandableNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        let (anchor_type, expanded) = (self.anchor_type, self.expanded);
        let editable = anchor_type == ExposedDropdownMenuAnchorType::PrimaryEditable;
        if anchor_type == ExposedDropdownMenuAnchorType::SecondaryEditable {
            config.set_role(Role::Button);
            config.set_state_description(if expanded { "Expanded" } else { "Collapsed" });
            config.set_content_description("Toggle dropdown menu");
        } else {
            config.set_role(Role::DropdownList);
        }
        let shared = self.shared.clone();
        config.on_click(None, move || {
            if editable {
                shared.focus_requester.request_focus();
            }
            shared.toggle(anchor_type, expanded);
            if editable && let Some(keyboard) = shared.keyboard.borrow().as_ref() {
                keyboard.show();
            }
            true
        });
    }
}

/// The largest height of a menu at `anchor` in a window down to `window_bottom`, as Compose's
/// `calculateMaxHeight`: the larger space beside it, or the window if it's scrolled out.
fn calculate_max_height(window_bottom: i32, anchor: Rect, vertical_margin: i32) -> i32 {
    let (top, bottom) = (vertical_margin, window_bottom - vertical_margin);
    let available = if anchor.top > window_bottom as f32 || anchor.bottom < 0.0 {
        bottom - top
    } else {
        (anchor.top - top as f32).max(bottom as f32 - anchor.bottom).round() as i32
    };
    available.max(0)
}

struct SizeElement {
    shared: Rc<Shared>,
    match_anchor_width: bool,
}

struct SizeNode {
    shared: Rc<Shared>,
    match_anchor_width: bool,
}

impl ModifierNodeElement for SizeElement {
    type Node = SizeNode;
    fn create(&self) -> SizeNode {
        SizeNode { shared: self.shared.clone(), match_anchor_width: self.match_anchor_width }
    }
    fn update(&self, node: &mut SizeNode) {
        node.shared = self.shared.clone();
        node.match_anchor_width = self.match_anchor_width;
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.shared, &other.shared)
            && self.match_anchor_width == other.match_anchor_width
    }
    fn name(&self) -> &'static str {
        "exposedDropdownSize"
    }
}

impl ModifierNode for SizeNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for SizeNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        // Read here, not as it's composed: a scroll or the keyboard measures the menu again.
        let anchor = self.shared.anchor_bounds.get();
        let window_bottom = scope.window_size().height - self.shared.ime.get_bottom(&density);
        let vertical_margin = density.round_to_px(MENU_VERTICAL_MARGIN);
        let max_height = calculate_max_height(window_bottom, anchor, vertical_margin);
        let menu_width = constraints.constrain_width((anchor.right - anchor.left) as i32);
        let placeable = scope.measure(
            content,
            Constraints {
                max_height: constraints.constrain_height(max_height),
                min_width: if self.match_anchor_width { menu_width } else { constraints.min_width },
                max_width: if self.match_anchor_width { menu_width } else { constraints.max_width },
                ..constraints
            },
        );
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

/// The menu of an `ExposedDropdownMenuBox`, as Compose's `ExposedDropdownMenu`: a `DropdownMenu`
/// that's at the box, as wide as it unless not `match_anchor_width`, and doesn't take the focus
/// from what's typed in unless the keyboard asks.
#[composable]
#[allow(non_snake_case)]
pub fn ExposedDropdownMenu(
    scope: ExposedDropdownMenuBoxScope,
    expanded: bool,
    on_dismiss_request: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_scroll_state(0))] scroll_state: compose_foundation::ScrollState,
    #[default(true)] match_anchor_width: bool,
    #[default(crate::MenuDefaults::shape())] shape: Shape,
    #[default(crate::MenuDefaults::container_color())] container_color: Color,
    #[default(crate::MenuDefaults::TONAL_ELEVATION)] tonal_elevation: Dp,
    #[default(crate::MenuDefaults::SHADOW_ELEVATION)] shadow_elevation: Dp,
    #[default(None)] border: Option<BorderStroke>,
    content: impl Fn(ColumnScope) + 'static,
) {
    let expanded_state = remember(|| MutableTransitionState::new(false));
    expanded_state.set_target_state(expanded);
    if expanded_state.current_state() || expanded_state.target_state() {
        let density = compose_ui::LOCAL_DENSITY.current();
        let position = compose_runtime::remember_keyed(density, || {
            // Not the status bars, unlike Compose's: here the window's height has them already.
            Rc::new(ExposedDropdownMenuPositionProvider::new(density, 0))
        });
        let shared = scope.shared.clone();
        let focus_kept = matches!(
            shared.anchor_type.get(),
            ExposedDropdownMenuAnchorType::PrimaryEditable
                | ExposedDropdownMenuAnchorType::SecondaryEditable
        ) && !shared.always_focusable.get();
        let properties = PopupProperties { focusable: !focus_kept, ..PopupProperties::default() };
        compose_ui::PopupWithPositionProvider(
            position.clone() as Rc<dyn PopupPositionProvider>,
            Some(on_dismiss_request),
            properties,
            compose_runtime::callback(move || {
                let position = position.clone();
                dropdown_menu_content(
                    &expanded_state,
                    move || position.transform_origin.get(),
                    modifier.clone().exposed_dropdown_size(&scope, match_anchor_width),
                    scroll_state,
                    shape.clone(),
                    container_color,
                    tonal_elevation,
                    shadow_elevation,
                    border.clone(),
                    content.clone(),
                );
            }),
        );
    }
}

/// Where a menu is at its anchor, as Compose's `ExposedDropdownMenuPositionProvider`: below it or
/// above it, else in the window, and never over the margins when it fits between them.
#[doc(hidden)]
pub struct ExposedDropdownMenuPositionProvider {
    top_window_insets: i32,
    vertical_margin: i32,
    transform_origin: Cell<TransformOrigin>,
}

impl ExposedDropdownMenuPositionProvider {
    pub fn new(density: Density, top_window_insets: i32) -> Self {
        ExposedDropdownMenuPositionProvider {
            top_window_insets,
            vertical_margin: density.round_to_px(MENU_VERTICAL_MARGIN),
            transform_origin: Cell::new(TransformOrigin::CENTER),
        }
    }
}

impl PopupPositionProvider for ExposedDropdownMenuPositionProvider {
    fn position(
        &self,
        anchor: Rect,
        window: IntSize,
        layout_direction: LayoutDirection,
        popup: IntSize,
    ) -> IntOffset {
        let window = IntSize::new(window.width, window.height + self.top_window_insets);
        let (left, right) = (anchor.left.round() as i32, anchor.right.round() as i32);
        let (top, bottom) = (anchor.top.round() as i32, anchor.bottom.round() as i32);
        let (start, end) = if layout_direction == LayoutDirection::Ltr {
            (left, right - popup.width)
        } else {
            (right - popup.width, left)
        };
        let by_window =
            if (left + right) / 2 < window.width / 2 { 0 } else { window.width - popup.width };
        let x = [start, end]
            .into_iter()
            .find(|&x| x >= 0 && x + popup.width <= window.width)
            .unwrap_or(by_window);
        let by_window =
            if (top + bottom) / 2 < window.height / 2 { 0 } else { window.height - popup.height };
        let margin = self.vertical_margin;
        let y = [bottom, top - popup.height]
            .into_iter()
            .find(|&y| y >= 0 && y + popup.height <= window.height)
            .unwrap_or_else(|| {
                if popup.height >= window.height - 2 * margin {
                    (window.height - popup.height) / 2
                } else {
                    by_window.clamp(margin, window.height - margin - popup.height)
                }
            });
        let position = IntOffset::new(x, y);
        let menu =
            Rect::new(x as f32, y as f32, (x + popup.width) as f32, (y + popup.height) as f32);
        self.transform_origin.set(transform_origin(anchor, menu));
        position
    }
}

/// As Compose's `ExposedDropdownMenuDefaults`.
pub struct ExposedDropdownMenuDefaults;

impl ExposedDropdownMenuDefaults {
    /// The padding of a `DropdownMenuItem` lining it up with the text field, as
    /// `ItemContentPadding`.
    pub const ITEM_CONTENT_PADDING: PaddingValues =
        PaddingValues { start: Dp(16.0), top: Dp(0.0), end: Dp(16.0), bottom: Dp(0.0) };

    /// The arrow, pointing up while `expanded`, as `TrailingIcon`.
    pub fn trailing_icon(expanded: bool, modifier: Modifier) {
        crate::icon::Icon(
            crate::icons::filled::arrow_drop_down(),
            None,
            modifier.rotate(if expanded { 180.0 } else { 0.0 }),
            crate::theme::LOCAL_CONTENT_COLOR.current(),
        );
    }

    /// A `TextField`'s colors in a box, as `textFieldColors()`: the container is the same when
    /// disabled (`FilledAutocompleteTokens`).
    pub fn text_field_colors() -> TextFieldColors {
        TextFieldColors {
            disabled_container_color: MaterialTheme::color_scheme().surface_container_highest,
            ..TextFieldDefaults::colors()
        }
    }

    /// An `OutlinedTextField`'s colors in a box, as `outlinedTextFieldColors()`.
    pub fn outlined_text_field_colors() -> TextFieldColors {
        crate::OutlinedTextFieldDefaults::colors()
    }
}
