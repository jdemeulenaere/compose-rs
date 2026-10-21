//! Input events: pointer, key and text, and what their handlers can do (docs/06-ui.md, "Input,
//! focus and semantics").

use compose_runtime::Callback;

use crate::tree::NodeId;
use crate::unit::Offset;

/// A pointer: the mouse, or a finger.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PointerId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum PointerType {
    #[default]
    Mouse,
    Touch,
    Stylus,
}

/// What happened, as `PointerEventType`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PointerEventType {
    Press,
    Release,
    Move,
    Scroll,
    /// A mouse came over the node (its hit path), as `PointerEventType.Enter`.
    Enter,
    /// A mouse left it, or the window, as `PointerEventType.Exit`.
    Exit,
}

/// The mouse pointer's look, as Compose's `PointerIcon`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum PointerIcon {
    /// The arrow.
    #[default]
    Default,
    /// The I-beam, over text that can be edited or selected.
    Text,
    /// A hand, over what can be clicked.
    Hand,
    Crosshair,
}

impl compose_runtime::ComposeArg for PointerIcon {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// One pointer event, in the receiving node's coordinates, as Compose's `PointerEvent` with a
/// single `PointerInputChange`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PointerEvent {
    pub kind: PointerEventType,
    pub pointer: PointerId,
    pub pointer_type: PointerType,
    /// The position in the receiving node's coordinates.
    pub position: Offset,
    /// Whether the pointer is down after this event (a button held, a finger touching).
    pub pressed: bool,
    /// The scroll amount of a `Scroll` event, in pixels.
    pub scroll_delta: Offset,
    /// Set by a handler that used the event: handlers after it should ignore it, as
    /// `PointerInputChange.consume()`.
    pub consumed: bool,
}

impl PointerEvent {
    fn new(kind: PointerEventType, position: Offset, pressed: bool) -> PointerEvent {
        PointerEvent {
            kind,
            pointer: PointerId(0),
            pointer_type: PointerType::Mouse,
            position,
            pressed,
            scroll_delta: Offset::ZERO,
            consumed: false,
        }
    }

    /// The primary mouse button pressed at `position`.
    pub fn press(position: Offset) -> PointerEvent {
        PointerEvent::new(PointerEventType::Press, position, true)
    }

    pub fn release(position: Offset) -> PointerEvent {
        PointerEvent::new(PointerEventType::Release, position, false)
    }

    /// The pointer moved to `position`, `pressed` or not.
    pub fn moved(position: Offset, pressed: bool) -> PointerEvent {
        PointerEvent::new(PointerEventType::Move, position, pressed)
    }

    pub fn scroll(position: Offset, delta: Offset) -> PointerEvent {
        PointerEvent {
            scroll_delta: delta,
            ..PointerEvent::new(PointerEventType::Scroll, position, false)
        }
    }

    /// The same event for another pointer (a finger).
    pub fn with_pointer(self, pointer: PointerId, pointer_type: PointerType) -> PointerEvent {
        PointerEvent { pointer, pointer_type, ..self }
    }

    pub fn consume(&mut self) {
        self.consumed = true;
    }
}

/// The passes of pointer dispatch, as `PointerEventPass`: `Initial` from the root down the hit
/// path, `Main` from the target up, `Final` down again.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PointerEventPass {
    Initial,
    Main,
    Final,
}

/// A key, as Compose's `Key`, for the keys the toolkit handles; other keys arrive as
/// `Character` or `Other`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    Enter,
    NumPadEnter,
    Space,
    Tab,
    Backspace,
    Delete,
    Escape,
    DirectionLeft,
    DirectionRight,
    DirectionUp,
    DirectionDown,
    MoveHome,
    MoveEnd,
    PageUp,
    PageDown,
    Character(char),
    Other(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum KeyEventType {
    KeyDown,
    KeyUp,
}

/// Modifier keys held during an event.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// A key event, as Compose's `KeyEvent`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyEvent {
    pub key: Key,
    pub kind: KeyEventType,
    pub modifiers: KeyModifiers,
}

impl KeyEvent {
    pub fn down(key: Key) -> KeyEvent {
        KeyEvent { key, kind: KeyEventType::KeyDown, modifiers: KeyModifiers::default() }
    }

    pub fn up(key: Key) -> KeyEvent {
        KeyEvent { key, kind: KeyEventType::KeyUp, modifiers: KeyModifiers::default() }
    }
}

/// Work a handler leaves for after dispatch, when the tree is free again: application
/// callbacks run then, so they can do anything a callback may.
pub enum Action {
    Call(Callback<dyn Fn()>),
    CallWithBool(Callback<dyn Fn(bool)>, bool),
    Run(Box<dyn FnOnce()>),
}

impl Action {
    pub fn run(self) {
        match self {
            Action::Call(callback) => callback(),
            Action::CallWithBool(callback, value) => callback(value),
            Action::Run(function) => function(),
        }
    }
}

/// What an event handler can do: queue actions, ask for focus, invalidate drawing or layout.
pub struct EventContext<'a> {
    pub(crate) node: NodeId,
    pub(crate) actions: &'a mut Vec<Action>,
    pub(crate) requests: &'a mut Requests,
    pub(crate) text_measurer: &'a mut dyn crate::text::TextMeasurer,
    pub(crate) clipboard: &'a mut dyn crate::clipboard::Clipboard,
    /// The pointer's position in the root, while handling a pointer event.
    pub(crate) pointer_position: Option<Offset>,
    pub(crate) density: crate::unit::Density,
    pub(crate) layout_direction: crate::alignment::LayoutDirection,
}

impl EventContext<'_> {
    /// The density, to turn dp distances (a touch slop) into pixels.
    pub fn density(&self) -> crate::unit::Density {
        self.density
    }

    /// The handling node's layout direction: a horizontal scroll goes the other way in a
    /// right-to-left layout.
    pub fn layout_direction(&self) -> crate::alignment::LayoutDirection {
        self.layout_direction
    }

    /// Where the pointer is in the window, while handling a pointer event.
    pub fn pointer_position(&self) -> Option<Offset> {
        self.pointer_position
    }
}

/// Requests handlers make, applied after dispatch.
#[derive(Default)]
pub(crate) struct Requests {
    pub(crate) focus: Option<NodeId>,
    /// Where (in the root) the tap that asked for focus was, for the focus target to use.
    pub(crate) focus_tap: Option<Offset>,
    pub(crate) clear_focus: bool,
    pub(crate) draw: bool,
    pub(crate) measure: Vec<NodeId>,
    pub(crate) reset_input_method: bool,
}

impl EventContext<'_> {
    /// Asks the platform to reset its input method: a text field committed or dropped the
    /// composition itself (a click, a focus change), so the input method mustn't commit it
    /// again.
    pub fn reset_input_method(&mut self) {
        self.requests.reset_input_method = true;
    }

    /// Runs `callback` after dispatch.
    pub fn call(&mut self, callback: &Callback<dyn Fn()>) {
        self.actions.push(Action::Call(callback.clone()));
    }

    pub fn call_with_bool(&mut self, callback: &Callback<dyn Fn(bool)>, value: bool) {
        self.actions.push(Action::CallWithBool(callback.clone(), value));
    }

    /// Runs `function` after dispatch.
    pub fn run(&mut self, function: impl FnOnce() + 'static) {
        self.actions.push(Action::Run(Box::new(function)));
    }

    /// Moves focus to this node's focus target, as `FocusRequester.requestFocus`.
    pub fn request_focus(&mut self) {
        self.requests.focus = Some(self.node);
    }

    /// Moves focus as `request_focus`, telling the focus target where the pointer was
    /// (`FocusTargetModifierNode::on_focus_tap`): a text field's decoration focuses the field and
    /// puts its cursor nearest the tap.
    pub fn request_focus_at_pointer(&mut self) {
        self.requests.focus = Some(self.node);
        self.requests.focus_tap = self.pointer_position;
    }

    /// Clears focus, as `FocusManager.clearFocus`.
    pub fn clear_focus(&mut self) {
        self.requests.clear_focus = true;
    }

    /// The node's drawing changed.
    pub fn invalidate_draw(&mut self) {
        self.requests.draw = true;
    }

    /// The node's measurement changed.
    pub fn invalidate_measure(&mut self) {
        self.requests.measure.push(self.node);
    }

    /// The text measurer, for handlers that hit-test their paragraphs (a text field's tap).
    pub fn text_measurer(&mut self) -> &mut dyn crate::text::TextMeasurer {
        &mut *self.text_measurer
    }

    /// The clipboard, for copy and paste.
    pub fn clipboard(&mut self) -> &mut dyn crate::clipboard::Clipboard {
        &mut *self.clipboard
    }

    /// The layout node the handler belongs to.
    pub fn node(&self) -> NodeId {
        self.node
    }
}
