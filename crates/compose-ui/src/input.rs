//! Input events: pointer, key and text, and what their handlers can do (docs/06-ui.md, "Input,
//! focus and semantics").

use std::rc::Rc;

use compose_runtime::Callback;

use crate::tree::NodeId;
use crate::unit::{IntSize, Offset};

/// A pointer: the mouse, or a finger.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PointerId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[non_exhaustive]
pub enum PointerType {
    #[default]
    Mouse,
    Touch,
    Stylus,
    /// An eraser, or a stylus held the other way up.
    Eraser,
    Unknown,
}

/// What happened, as `PointerEventType`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum PointerEventType {
    Press,
    Release,
    Move,
    Scroll,
    /// A mouse came over the node (its hit path), as `PointerEventType.Enter`.
    Enter,
    /// A mouse left it, or the window, as `PointerEventType.Exit`.
    Exit,
    /// A trackpad's pinch begins, changes the scale by the change's `scale_factor`, and ends, as
    /// `ScaleStart`, `ScaleChange` and `ScaleEnd`: the pointer stays where the cursor is.
    ScaleStart,
    ScaleChange,
    ScaleEnd,
    /// A trackpad's two-finger pan begins, moves by the change's `pan_offset`, and ends, as
    /// `PanStart`, `PanMove` and `PanEnd`: the pointer stays where the cursor is, hovering.
    PanStart,
    PanMove,
    PanEnd,
    Unknown,
}

impl PointerEventType {
    /// A trackpad's pan: `PanStart`, `PanMove` or `PanEnd`.
    pub fn is_pan(self) -> bool {
        matches!(
            self,
            PointerEventType::PanStart | PointerEventType::PanMove | PointerEventType::PanEnd
        )
    }

    /// A trackpad's pinch: `ScaleStart`, `ScaleChange` or `ScaleEnd`.
    pub fn is_scale(self) -> bool {
        matches!(
            self,
            PointerEventType::ScaleStart
                | PointerEventType::ScaleChange
                | PointerEventType::ScaleEnd
        )
    }
}

/// What a unit of a `Scroll` event's delta is worth in the scrollable it reaches, as Compose's
/// `ScrollConfig`: a host's wheel has its platform's step (D1450).
/// The desktop variants are **unverified** against Compose Desktop, whose source isn't in
/// androidx-main.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum WheelStep {
    /// The delta is in pixels: a trackpad's, or a platform that scaled it (Android's view).
    #[default]
    Pixels,
    /// A number of dp a unit: Compose Desktop's macOS, 10 dp.
    Dp(f32),
    /// The scrollable's height over 20, times the system's lines per notch: its Windows.
    HeightOverTwenty { scroll_amount: f32 },
    /// The square root of the scrollable's height, times the system's lines per notch: its Linux.
    SquareRootOfHeight { scroll_amount: f32 },
}

impl WheelStep {
    /// Pixels a unit, for a scrollable of size `bounds`.
    pub fn pixels_per_unit(self, density: f32, bounds: IntSize) -> f32 {
        let height = bounds.height as f32;
        match self {
            WheelStep::Pixels => 1.0,
            WheelStep::Dp(dp) => dp * density,
            WheelStep::HeightOverTwenty { scroll_amount } => height / 20.0 * scroll_amount,
            WheelStep::SquareRootOfHeight { scroll_amount } => height.sqrt() * scroll_amount,
        }
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
    /// The buttons during it.
    pub buttons: crate::pointer_event::PointerButtons,
    /// The modifier keys during it.
    pub keyboard_modifiers: crate::pointer_event::PointerKeyboardModifiers,
    pub pressure: f32,
    /// The scroll amount of a `Scroll` event: pixels, or the wheel's own units as `wheel_step` says.
    pub scroll_delta: Offset,
    /// What a unit of `scroll_delta` is worth in the scrollable it reaches (D1450).
    pub wheel_step: WheelStep,
    /// A `Scroll` event from a high-resolution wheel or a trackpad, following the fingers: it
    /// applies at once, not animated, as Compose's `isPreciseWheelScroll`.
    pub precise_wheel: bool,
    /// How far a trackpad's `PanMove` pans, in pixels, as `PointerInputChange.panOffset`.
    pub pan_offset: Offset,
    /// How much a trackpad's `ScaleChange` scales (1 is no change), as
    /// `PointerInputChange.scaleFactor`.
    pub scale_factor: f32,
    /// Set by a handler that used the event: handlers after it should ignore it, as
    /// `PointerInputChange.consume()`.
    pub consumed: bool,
    /// When it happened, by the executor's clock (nanoseconds), as `uptimeMillis`: a platform
    /// delivering events late or batched says when each was; 0 is "as it's dispatched".
    pub time_nanos: u64,
}

impl PointerEvent {
    /// Its kind, a held pointer's `Enter` or `Exit` taken as the move it is: a held mouse's move
    /// that comes over or leaves a node is that node's `Enter` or `Exit`, as Compose's hit path
    /// makes it (D2361), and what follows a press or a drag follows the pointer out and back, as
    /// Compose's gesture detectors, which look at the changes, not the kind.
    pub fn kind_moving(&self) -> PointerEventType {
        match self.kind {
            PointerEventType::Enter | PointerEventType::Exit if self.pressed => {
                PointerEventType::Move
            }
            kind => kind,
        }
    }

    fn new(kind: PointerEventType, position: Offset, pressed: bool) -> PointerEvent {
        PointerEvent {
            kind,
            pointer: PointerId(0),
            pointer_type: PointerType::Mouse,
            position,
            pressed,
            buttons: if pressed {
                crate::pointer_event::PointerButtons::PRIMARY
            } else {
                crate::pointer_event::PointerButtons::NONE
            },
            keyboard_modifiers: Default::default(),
            pressure: 1.0,
            scroll_delta: Offset::ZERO,
            wheel_step: WheelStep::Pixels,
            precise_wheel: false,
            pan_offset: Offset::ZERO,
            scale_factor: 1.0,
            consumed: false,
            time_nanos: 0,
        }
    }

    /// A trackpad's pan event (`PanStart`, `PanMove` or `PanEnd`) where the cursor is, panning by
    /// `offset`.
    pub fn pan(kind: PointerEventType, position: Offset, offset: Offset) -> PointerEvent {
        debug_assert!(kind.is_pan());
        PointerEvent { pan_offset: offset, ..PointerEvent::new(kind, position, false) }
    }

    /// A trackpad's pinch event (`ScaleStart`, `ScaleChange` or `ScaleEnd`) where the cursor is,
    /// scaling by `factor`.
    pub fn scale(kind: PointerEventType, position: Offset, factor: f32) -> PointerEvent {
        debug_assert!(kind.is_scale());
        PointerEvent { scale_factor: factor, ..PointerEvent::new(kind, position, false) }
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

    /// The same `Scroll` event, its delta in the units of `step`.
    pub fn with_wheel_step(self, step: WheelStep) -> PointerEvent {
        PointerEvent { wheel_step: step, ..self }
    }

    /// The pixels a `Scroll` event is worth to a scrollable of size `bounds`, as Compose's
    /// `ScrollConfig.calculateMouseWheelScroll`.
    pub fn wheel_pixels(&self, density: f32, bounds: IntSize) -> Offset {
        let scale = self.wheel_step.pixels_per_unit(density, bounds);
        Offset::new(self.scroll_delta.x * scale, self.scroll_delta.y * scale)
    }

    /// The same `Scroll` event from a high-resolution wheel or a trackpad, if `precise`.
    pub fn with_precise_wheel(self, precise: bool) -> PointerEvent {
        PointerEvent { precise_wheel: precise, ..self }
    }

    /// The same event, as it happened at `time_nanos` (by the executor's clock).
    pub fn at_time(self, time_nanos: u64) -> PointerEvent {
        PointerEvent { time_nanos, ..self }
    }

    /// The same event for another pointer (a finger): only a mouse has buttons by default.
    pub fn with_pointer(self, pointer: PointerId, pointer_type: PointerType) -> PointerEvent {
        let buttons =
            if pointer_type == PointerType::Mouse { self.buttons } else { Default::default() };
        PointerEvent { pointer, pointer_type, buttons, ..self }
    }

    /// The same event with `buttons` held after it: the pointer's down as long as one is, as a mouse
    /// is. A press or release of one of several stays a `Press` or `Release`.
    pub fn with_buttons(self, buttons: crate::pointer_event::PointerButtons) -> PointerEvent {
        let pressed = buttons.are_any_pressed();
        PointerEvent { buttons, pressed, ..self }
    }

    /// The same event with the modifier keys `modifiers` held.
    pub fn with_keyboard_modifiers(
        self,
        keyboard_modifiers: crate::pointer_event::PointerKeyboardModifiers,
    ) -> PointerEvent {
        PointerEvent { keyboard_modifiers, ..self }
    }

    /// The same event at `pressure`.
    pub fn with_pressure(self, pressure: f32) -> PointerEvent {
        PointerEvent { pressure, ..self }
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
#[non_exhaustive]
pub enum Key {
    Enter,
    NumPadEnter,
    Space,
    /// The D-pad's center, as `Key.DirectionCenter`.
    DirectionCenter,
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
    /// The back key, as `Key.Back`.
    Back,
    /// Keyboard navigation's own keys, as `Key.NavigateNext` and `Key.NavigatePrevious`.
    NavigateNext,
    NavigatePrevious,
    /// A keyboard's own clipboard keys and Insert, as `Key.Copy`, `Key.Cut`, `Key.Paste` and
    /// `Key.Insert`.
    Copy,
    Cut,
    Paste,
    Insert,
    /// The numeric keypad's navigation keys (Num Lock off), as `Key.NumPadDirectionUp` and the
    /// others: a desktop's; Android has none.
    NumPadDirectionUp,
    NumPadDirectionDown,
    NumPadDirectionLeft,
    NumPadDirectionRight,
    NumPadMoveHome,
    NumPadMoveEnd,
    NumPadPageUp,
    NumPadPageDown,
    NumPadInsert,
    /// The modifier keys, as Compose's `Key.ShiftLeft`… A host reports what they hold in
    /// [`KeyEvent::modifiers`]; tests' key injection presses them as keys (D903).
    ShiftLeft,
    ShiftRight,
    CtrlLeft,
    CtrlRight,
    AltLeft,
    AltRight,
    MetaLeft,
    MetaRight,
    /// The lock keys, as `Key.CapsLock`, `Key.NumLock`, `Key.ScrollLock`.
    CapsLock,
    NumLock,
    ScrollLock,
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
    /// An on-screen keyboard sent it (Android's `FLAG_SOFT_KEYBOARD`, as Compose's
    /// `isFromSoftKeyboard`): a text field's edit by it doesn't restart the input method.
    pub from_soft_keyboard: bool,
}

impl KeyEvent {
    /// The direction this key moves the focus in, if nothing takes it, as Compose's
    /// `toFocusDirection`.
    pub fn to_focus_direction(&self) -> Option<crate::focus_manager::FocusDirection> {
        use crate::focus_manager::FocusDirection as Direction;
        Some(match self.key {
            Key::NavigatePrevious => Direction::Previous,
            Key::NavigateNext => Direction::Next,
            Key::Tab if self.modifiers.shift => Direction::Previous,
            Key::Tab => Direction::Next,
            Key::DirectionRight => Direction::Right,
            Key::DirectionLeft => Direction::Left,
            Key::DirectionUp | Key::PageUp => Direction::Up,
            Key::DirectionDown | Key::PageDown => Direction::Down,
            Key::DirectionCenter | Key::Enter | Key::NumPadEnter => Direction::Enter,
            Key::Back | Key::Escape => Direction::Exit,
            _ => return None,
        })
    }

    /// The key a numeric keypad's navigation key stands for (its arrows, Home, End, Page Up and
    /// Down, Insert), as Compose's key mappings take either; any other key as it is.
    pub fn without_num_pad(key: Key) -> Key {
        match key {
            Key::NumPadDirectionUp => Key::DirectionUp,
            Key::NumPadDirectionDown => Key::DirectionDown,
            Key::NumPadDirectionLeft => Key::DirectionLeft,
            Key::NumPadDirectionRight => Key::DirectionRight,
            Key::NumPadMoveHome => Key::MoveHome,
            Key::NumPadMoveEnd => Key::MoveEnd,
            Key::NumPadPageUp => Key::PageUp,
            Key::NumPadPageDown => Key::PageDown,
            Key::NumPadInsert => Key::Insert,
            other => other,
        }
    }

    pub fn down(key: Key) -> KeyEvent {
        KeyEvent {
            key,
            kind: KeyEventType::KeyDown,
            modifiers: KeyModifiers::default(),
            from_soft_keyboard: false,
        }
    }

    pub fn up(key: Key) -> KeyEvent {
        KeyEvent {
            key,
            kind: KeyEventType::KeyUp,
            modifiers: KeyModifiers::default(),
            from_soft_keyboard: false,
        }
    }
}

/// Work a handler leaves for after dispatch, when the tree is free again: application
/// callbacks run then, so they can do anything a callback may.
pub enum Action {
    Call(Callback<dyn Fn()>),
    CallWithBool(Callback<dyn Fn(bool)>, bool),
    Run(Box<dyn FnOnce()>),
    /// A focus event for application code: not delivered if a callback of its batch moved the
    /// focus, as Compose's transactions stop telling then (`run_actions`); nor, with `last`
    /// (`on_focus_changed`), if it's what was delivered last.
    FocusEvent {
        callback: Callback<dyn Fn(crate::focus::FocusState)>,
        state: crate::focus::FocusState,
        last: Option<Rc<std::cell::Cell<Option<crate::focus::FocusState>>>>,
        focus: FocusBatch,
    },
    /// The `on_focus_change` of a `FocusTargetNode`, dropped as `FocusEvent`.
    FocusChange {
        callback: Callback<dyn Fn(crate::focus::FocusState, crate::focus::FocusState)>,
        previous: crate::focus::FocusState,
        current: crate::focus::FocusState,
        focus: FocusBatch,
    },
}

/// Where a focus action's batch started: its generation of the focus.
pub struct FocusBatch {
    generation: crate::focus::FocusGeneration,
    started_at: Option<u64>,
    /// Queued by `performRequestFocus`, whose telling stops if a callback moves the focus: a
    /// clear's goes on.
    droppable: bool,
}

impl FocusBatch {
    /// A callback of the batch moved the focus.
    fn is_stale(&self) -> bool {
        self.started_at.is_some_and(|started_at| started_at != self.generation.get())
    }
}

/// Runs `actions`, what a dispatch or a focus transaction left, in order: the focus actions among
/// them that follow one moving the focus aren't delivered.
pub(crate) fn run_actions(mut actions: Vec<Action>) {
    for action in &mut actions {
        if let Action::FocusEvent { focus, .. } | Action::FocusChange { focus, .. } = action
            && focus.droppable
        {
            focus.started_at = Some(focus.generation.get());
        }
    }
    for action in actions {
        action.run();
    }
}

impl Action {
    pub fn run(self) {
        match self {
            Action::Call(callback) => callback(),
            Action::CallWithBool(callback, value) => callback(value),
            Action::Run(function) => function(),
            Action::FocusChange { callback, previous, current, focus } => {
                if !focus.is_stale() {
                    callback(previous, current);
                }
            }
            Action::FocusEvent { callback, state, last, focus } => {
                if focus.is_stale() {
                    return;
                }
                if let Some(last) = last {
                    if last.get() == Some(state) {
                        return;
                    }
                    last.set(Some(state));
                }
                callback(state);
            }
        }
    }
}

/// What an event handler can do: queue actions, ask for focus, invalidate drawing or layout.
pub struct EventContext<'a> {
    /// The handling node's own, as design §3.3: through it a handler traverses, gets a handle,
    /// observes reads (`Deref`).
    pub(crate) context: crate::node_context::NodeContext<'a>,
    pub(crate) actions: &'a mut Vec<Action>,
    pub(crate) requests: &'a mut Requests,
    /// The pointer's position in the root, while handling a pointer event.
    pub(crate) pointer_position: Option<Offset>,
    /// Whether something around the handling node (a scrollable) waits for a gesture past a
    /// press, while handling a press, as Compose's `hasWaitingParent`.
    pub(crate) waiting_parent: bool,
}

impl<'a> std::ops::Deref for EventContext<'a> {
    type Target = crate::node_context::NodeContext<'a>;
    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl std::ops::DerefMut for EventContext<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.context
    }
}

impl EventContext<'_> {
    /// The density, to turn dp distances (a touch slop) into pixels.
    pub fn density(&self) -> crate::unit::Density {
        self.context.require_density()
    }

    /// The gesture timeouts and distances where its layout node is composed, as a node's
    /// `currentValueOf(LocalViewConfiguration)`.
    pub fn view_configuration(&self) -> crate::view_configuration::ViewConfiguration {
        self.context.tree.view_configuration_of(self.context.node)
    }

    /// As `firstDownRefersToPrimaryMouseButtonOnly`, the host's.
    pub fn first_down_primary_only(&self) -> bool {
        self.context.tree.first_down_primary_only
    }

    /// How far past a node of `bounds` a touch still counts as on it, as `getExtendedTouchPadding`.
    pub fn extended_touch_padding(&self, bounds: IntSize) -> crate::unit::Size {
        let minimum = self.view_configuration().minimum_touch_target_size;
        let density = self.density();
        crate::unit::Size::new(
            ((density.to_px(minimum.width) - bounds.width as f32) / 2.0).max(0.0),
            ((density.to_px(minimum.height) - bounds.height as f32) / 2.0).max(0.0),
        )
    }

    /// The handling node's layout direction: a horizontal scroll goes the other way in a
    /// right-to-left layout.
    pub fn layout_direction(&self) -> crate::alignment::LayoutDirection {
        self.context.require_layout_direction()
    }

    /// Whether something around the handling node waits for a gesture past the press being
    /// handled (a scrollable for its slop), as Compose's `hasWaitingParent`: a clickable then
    /// delays showing the press by `TAP_INDICATION_DELAY`.
    pub fn has_waiting_parent(&self) -> bool {
        self.waiting_parent
    }

    /// Where the pointer is in the window, while handling a pointer event.
    pub fn pointer_position(&self) -> Option<Offset> {
        self.pointer_position
    }
}

/// A node asking to be shown, with what of it: a rect in a chain entry's layer, or all of it.
pub(crate) type BringIntoViewRequest = (NodeId, Option<(usize, crate::unit::Rect)>);

/// A dispatch's requests to be shown: most have none, a text field gaining the focus two (its
/// cursor and its decoration), so they're kept inline (D1149).
pub(crate) type BringIntoViewRequests = smallvec::SmallVec<[BringIntoViewRequest; 2]>;

/// Requests handlers make, applied after dispatch.
#[derive(Default)]
pub(crate) struct Requests {
    pub(crate) focus: Option<NodeId>,
    /// Where (in the root) the tap that asked for focus was, for the focus target to use.
    pub(crate) focus_tap: Option<Offset>,
    pub(crate) clear_focus: bool,
    /// Tab's move, forward or back.
    pub(crate) move_focus: Option<bool>,
    /// Text selection gestures and commands.
    pub(crate) selection: Vec<crate::text_selection::SelectionRequest>,
    /// Nodes whose drawing changed.
    pub(crate) draw: Vec<NodeId>,
    pub(crate) measure: Vec<NodeId>,
    pub(crate) reset_input_method: bool,
    /// Show (true) or hide (false) the on-screen keyboard.
    pub(crate) software_keyboard: Option<bool>,
    /// The handling nodes asking to be shown (`EventContext::bring_into_view`), each with what
    /// of it: a rect in its chain entry's layer (`EventContext::bring_rect_into_view`), or all of
    /// it. Shown together, as Compose's `ContentInViewNode` queues them (D1145).
    pub(crate) bring_into_view: BringIntoViewRequests,
    /// A hover icon node was hovered (D1676).
    pub(crate) hover_icons: bool,
}

impl EventContext<'_> {
    /// Asks the platform to reset its input method: a text field committed or dropped the
    /// composition itself (a click, a focus change), so the input method mustn't commit it
    /// again.
    pub fn reset_input_method(&mut self) {
        self.requests.reset_input_method = true;
    }

    /// Runs `callback` after dispatch.
    /// Whether `event`'s position is outside `bounds`, extended to the minimum touch target
    /// around a smaller node, as `isOutOfBounds(size, extendedTouchPadding)`, the target's
    /// `LOCAL_VIEW_CONFIGURATION`'s `minimum_touch_target_size`.
    pub fn is_out_of_touch_target(&self, event: &PointerEvent, bounds: IntSize) -> bool {
        let minimum = self.view_configuration().minimum_touch_target_size;
        let density = self.density();
        let horizontal = ((density.to_px(minimum.width) - bounds.width as f32) / 2.0).max(0.0);
        let vertical = ((density.to_px(minimum.height) - bounds.height as f32) / 2.0).max(0.0);
        let position = event.position;
        position.x < -horizontal
            || position.y < -vertical
            || position.x > bounds.width as f32 + horizontal
            || position.y > bounds.height as f32 + vertical
    }

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

    /// Asks the scrolling containers around this node to show it, as a focused node does
    /// (`FocusableNode`'s `bringIntoView`): once it's placed, if it isn't yet.
    pub fn bring_into_view(&mut self) {
        self.request_bring_into_view(None);
    }

    /// Asks the scrolling containers around this node to show `rect`, in the coordinates of the
    /// layout this node is in, as a focused text field shows its cursor (Compose's
    /// `bringSelectionEndIntoView`): once it's placed, if it isn't yet (all of it, then).
    pub fn bring_rect_into_view(&mut self, rect: crate::unit::Rect) {
        self.request_bring_into_view(self.context.index.map(|index| (index, rect)));
    }

    /// A node's request takes the place of its last one; requests of several nodes in one focus
    /// change are shown together, the largest that fits each container (D1145).
    fn request_bring_into_view(&mut self, what: Option<(usize, crate::unit::Rect)>) {
        let node = self.context.node;
        let requests = &mut self.requests.bring_into_view;
        match requests.iter_mut().find(|(each, _)| *each == node) {
            Some(request) => request.1 = what,
            None => requests.push((node, what)),
        }
    }

    /// Queues a focus event for application code (`on_focus_event`, `on_focus_changed`), as
    /// `Action::FocusEvent`.
    pub(crate) fn focus_event_action(
        &mut self,
        callback: Callback<dyn Fn(crate::focus::FocusState)>,
        state: crate::focus::FocusState,
        last: Option<Rc<std::cell::Cell<Option<crate::focus::FocusState>>>>,
    ) {
        let generation = self.context.tree.focus_requests.focus_generation.clone();
        let droppable = self.context.tree.focus.requesting;
        let focus = FocusBatch { generation, started_at: None, droppable };
        self.actions.push(Action::FocusEvent { callback, state, last, focus });
    }

    /// Queues the `on_focus_change` of a `FocusTargetNode`, as `Action::FocusChange`.
    pub(crate) fn focus_change_action(
        &mut self,
        callback: Callback<dyn Fn(crate::focus::FocusState, crate::focus::FocusState)>,
        previous: crate::focus::FocusState,
        current: crate::focus::FocusState,
    ) {
        let generation = self.context.tree.focus_requests.focus_generation.clone();
        let droppable = self.context.tree.focus.requesting;
        let focus = FocusBatch { generation, started_at: None, droppable };
        self.actions.push(Action::FocusChange { callback, previous, current, focus });
    }

    /// Moves focus to this node's focus target, as `FocusRequester.requestFocus`.
    pub fn request_focus(&mut self) {
        self.requests.focus = Some(self.context.node);
    }

    /// Moves focus as `request_focus`, telling the focus target where the pointer was
    /// (`FocusTargetModifierNode::on_focus_tap`): a text field's decoration focuses the field and
    /// puts its cursor nearest the tap.
    pub fn request_focus_at_pointer(&mut self) {
        self.requests.focus = Some(self.context.node);
        self.requests.focus_tap = self.pointer_position;
    }

    /// Moves focus to the next (or previous) focus target, as `FocusManager.moveFocus(Next)`
    /// (`Previous`).
    pub fn move_focus(&mut self, forward: bool) {
        self.requests.move_focus = Some(forward);
    }

    /// Clears focus, as `FocusManager.clearFocus`.
    pub fn clear_focus(&mut self) {
        self.requests.clear_focus = true;
    }

    /// Asks the platform to show its on-screen keyboard, as `SoftwareKeyboardController.show`.
    pub fn show_software_keyboard(&mut self) {
        self.requests.software_keyboard = Some(true);
    }

    /// Asks the platform to hide its on-screen keyboard, as `SoftwareKeyboardController.hide`
    /// (a text field's default `Done` action).
    pub fn hide_software_keyboard(&mut self) {
        self.requests.software_keyboard = Some(false);
    }

    /// The node's drawing changed: what it draws in (its repaint boundary) draws again.
    pub fn invalidate_draw(&mut self) {
        self.requests.draw.push(self.context.node);
    }

    /// The node's measurement changed.
    pub fn invalidate_measure(&mut self) {
        self.requests.measure.push(self.context.node);
    }

    /// The text measurer, for handlers that hit-test their paragraphs (a text field's tap).
    pub fn text_measurer(&mut self) -> std::cell::RefMut<'_, dyn crate::text::TextMeasurer> {
        self.context.tree.text_measurer.borrow_mut()
    }

    /// The clipboard, for copy and paste.
    pub fn clipboard(&self) -> &dyn crate::clipboard::ClipboardManager {
        &*self.context.tree.clipboard
    }

    /// The layout node the handler belongs to.
    pub fn node(&self) -> NodeId {
        self.context.node
    }
}

#[cfg(test)]
mod wheel_step_tests {
    use super::*;

    #[test]
    fn a_turns_pixels_scale_by_density_for_dp_and_by_height_otherwise() {
        let bounds = IntSize::new(50, 400);
        let event =
            |step| PointerEvent::scroll(Offset::ZERO, Offset::new(1.0, -2.0)).with_wheel_step(step);
        let pixels = |step| event(step).wheel_pixels(3.0, bounds);
        assert_eq!(pixels(WheelStep::Pixels), Offset::new(1.0, -2.0));
        assert_eq!(pixels(WheelStep::Dp(10.0)), Offset::new(30.0, -60.0));
        assert_eq!(
            pixels(WheelStep::HeightOverTwenty { scroll_amount: 3.0 }),
            Offset::new(60.0, -120.0)
        );
        assert_eq!(
            pixels(WheelStep::SquareRootOfHeight { scroll_amount: 2.0 }),
            Offset::new(40.0, -80.0)
        );
    }
}
