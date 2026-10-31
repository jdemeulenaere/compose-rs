//! What a `pointer_input` block hears, as Compose's `PointerEvent`: every pointer's change of one
//! platform event together, the state of the buttons and the keyboard's modifier keys during it.
//!
//! A host sends `input::PointerEvent`s, each one pointer's; those of one platform event go together
//! (`UiHost::send_pointers`) and each node hears the changes of the pointers that hit it (D1657).

use std::cell::Cell;
use std::rc::Rc;

use crate::input::{KeyModifiers, PointerEventType, PointerId, PointerType};
use crate::unit::{IntSize, Offset, Size};

/// The state of a mouse's or a stylus's buttons, as `PointerButtons`: Android's `MotionEvent` bits,
/// which a desktop's buttons are mapped to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct PointerButtons(pub i32);

const BUTTON_PRIMARY: i32 = 1;
const BUTTON_SECONDARY: i32 = 1 << 1;
const BUTTON_TERTIARY: i32 = 1 << 2;
const BUTTON_BACK: i32 = 1 << 3;
const BUTTON_FORWARD: i32 = 1 << 4;
const BUTTON_STYLUS_PRIMARY: i32 = 1 << 5;
const BUTTON_STYLUS_SECONDARY: i32 = 1 << 6;
const STYLUS_BUTTONS: i32 = BUTTON_STYLUS_PRIMARY | BUTTON_STYLUS_SECONDARY;

impl PointerButtons {
    pub const NONE: PointerButtons = PointerButtons(0);
    pub const PRIMARY: PointerButtons = PointerButtons(BUTTON_PRIMARY);
    pub const SECONDARY: PointerButtons = PointerButtons(BUTTON_SECONDARY);
    pub const TERTIARY: PointerButtons = PointerButtons(BUTTON_TERTIARY);
    pub const BACK: PointerButtons = PointerButtons(BUTTON_BACK);
    pub const FORWARD: PointerButtons = PointerButtons(BUTTON_FORWARD);

    pub fn is_primary_pressed(self) -> bool {
        self.0 & (BUTTON_PRIMARY | BUTTON_STYLUS_PRIMARY) != 0
    }

    pub fn is_secondary_pressed(self) -> bool {
        self.0 & (BUTTON_SECONDARY | BUTTON_STYLUS_SECONDARY) != 0
    }

    pub fn is_tertiary_pressed(self) -> bool {
        self.0 & BUTTON_TERTIARY != 0
    }

    pub fn is_back_pressed(self) -> bool {
        self.0 & BUTTON_BACK != 0
    }

    pub fn is_forward_pressed(self) -> bool {
        self.0 & BUTTON_FORWARD != 0
    }

    /// Whether the button at `index` is pressed, those with no purpose of their own too, as
    /// `isPressed(buttonIndex)`.
    pub fn is_pressed(self, index: u32) -> bool {
        match index {
            0 => self.is_primary_pressed(),
            1 => self.is_secondary_pressed(),
            2..=4 => self.0 & (1 << index) != 0,
            // Past the stylus's two.
            _ => index + 2 < 32 && self.0 & (1 << (index + 2)) != 0,
        }
    }

    pub fn are_any_pressed(self) -> bool {
        self.0 != 0
    }

    /// The stylus's primary and secondary as the primary and secondary.
    fn by_index(self) -> u32 {
        (((self.0 & STYLUS_BUTTONS) as u32) >> 5) | ((self.0 & !STYLUS_BUTTONS) as u32)
    }

    /// The index of the first button pressed, as `is_pressed` takes it, or -1.
    pub fn index_of_first_pressed(self) -> i32 {
        match self.by_index() {
            0 => -1,
            buttons => buttons.trailing_zeros() as i32,
        }
    }

    /// The index of the last button pressed, as `is_pressed` takes it, or -1.
    pub fn index_of_last_pressed(self) -> i32 {
        match self.by_index() {
            0 => -1,
            buttons => (31 - buttons.leading_zeros()) as i32,
        }
    }
}

/// The state of the modifier and lock keys, as `PointerKeyboardModifiers`: Android's `KeyEvent`
/// meta state bits, which a desktop's keys are mapped to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct PointerKeyboardModifiers(pub i32);

const META_SHIFT_ON: i32 = 0x1;
const META_ALT_ON: i32 = 0x2;
const META_SYM_ON: i32 = 0x4;
const META_FUNCTION_ON: i32 = 0x8;
const META_CTRL_ON: i32 = 0x1000;
const META_META_ON: i32 = 0x10000;
const META_CAPS_LOCK_ON: i32 = 0x100000;
const META_NUM_LOCK_ON: i32 = 0x200000;
const META_SCROLL_LOCK_ON: i32 = 0x400000;

impl PointerKeyboardModifiers {
    pub fn is_ctrl_pressed(self) -> bool {
        self.0 & META_CTRL_ON != 0
    }

    pub fn is_meta_pressed(self) -> bool {
        self.0 & META_META_ON != 0
    }

    pub fn is_alt_pressed(self) -> bool {
        self.0 & META_ALT_ON != 0
    }

    /// As Android's: it has no such state to tell.
    pub fn is_alt_graph_pressed(self) -> bool {
        false
    }

    pub fn is_sym_pressed(self) -> bool {
        self.0 & META_SYM_ON != 0
    }

    pub fn is_shift_pressed(self) -> bool {
        self.0 & META_SHIFT_ON != 0
    }

    pub fn is_function_pressed(self) -> bool {
        self.0 & META_FUNCTION_ON != 0
    }

    pub fn is_caps_lock_on(self) -> bool {
        self.0 & META_CAPS_LOCK_ON != 0
    }

    pub fn is_scroll_lock_on(self) -> bool {
        self.0 & META_SCROLL_LOCK_ON != 0
    }

    pub fn is_num_lock_on(self) -> bool {
        self.0 & META_NUM_LOCK_ON != 0
    }
}

impl From<KeyModifiers> for PointerKeyboardModifiers {
    fn from(keys: KeyModifiers) -> PointerKeyboardModifiers {
        let bit = |on: bool, bit: i32| if on { bit } else { 0 };
        PointerKeyboardModifiers(
            bit(keys.shift, META_SHIFT_ON)
                | bit(keys.ctrl, META_CTRL_ON)
                | bit(keys.alt, META_ALT_ON)
                | bit(keys.meta, META_META_ON),
        )
    }
}

/// A pointer's move between the last two events, as `HistoricalChange`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct HistoricalChange {
    pub uptime_millis: u64,
    pub position: Offset,
    pub scale_factor: f32,
    pub pan_offset: Offset,
}

/// A change of one pointer, as Compose's `PointerInputChange`. Its copies (`clone`, as `copy`) share
/// whether it's consumed; the fields can then be changed.
#[derive(Clone)]
pub struct PointerInputChange {
    pub id: PointerId,
    /// When it happened, by the executor's clock.
    pub uptime_millis: u64,
    /// Where it is in the node hearing it: it may be outside it.
    pub position: Offset,
    /// Whether a finger touches or a button is down.
    pub pressed: bool,
    pub pressure: f32,
    pub previous_uptime_millis: u64,
    /// Where it was at its last event, in the node as it's laid out now: its own moves in between
    /// don't count.
    pub previous_position: Offset,
    pub previous_pressed: bool,
    /// What it is, as `type`.
    pub pointer_type: PointerType,
    pub scroll_delta: Offset,
    pub scale_factor: f32,
    pub pan_offset: Offset,
    pub historical: Vec<HistoricalChange>,
    consumed: Rc<Cell<bool>>,
}

impl PointerInputChange {
    /// A change of a touch, as `PointerInputChange(id, uptimeMillis, position, pressed, …)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: PointerId,
        uptime_millis: u64,
        position: Offset,
        pressed: bool,
        previous_uptime_millis: u64,
        previous_position: Offset,
        previous_pressed: bool,
        is_initially_consumed: bool,
    ) -> PointerInputChange {
        PointerInputChange {
            id,
            uptime_millis,
            position,
            pressed,
            pressure: 1.0,
            previous_uptime_millis,
            previous_position,
            previous_pressed,
            pointer_type: PointerType::Touch,
            scroll_delta: Offset::ZERO,
            scale_factor: 1.0,
            pan_offset: Offset::ZERO,
            historical: Vec::new(),
            consumed: Rc::new(Cell::new(is_initially_consumed)),
        }
    }

    /// A change sharing `consumed` with what's dispatching it.
    pub(crate) fn sharing(mut self, consumed: Rc<Cell<bool>>) -> PointerInputChange {
        self.consumed = consumed;
        self
    }

    /// Marks it used: handlers after this one should leave it, as `consume()`.
    pub fn consume(&self) {
        self.consumed.set(true);
    }

    pub fn is_consumed(&self) -> bool {
        self.consumed.get()
    }

    /// A pointer coming down, if that's not consumed, as `changedToDown()`.
    pub fn changed_to_down(&self) -> bool {
        !self.is_consumed() && self.changed_to_down_ignore_consumed()
    }

    pub fn changed_to_down_ignore_consumed(&self) -> bool {
        !self.previous_pressed && self.pressed
    }

    /// A pointer coming up, if that's not consumed, as `changedToUp()`.
    pub fn changed_to_up(&self) -> bool {
        !self.is_consumed() && self.changed_to_up_ignore_consumed()
    }

    pub fn changed_to_up_ignore_consumed(&self) -> bool {
        self.previous_pressed && !self.pressed
    }

    /// How far it moved, nothing if that's consumed, as `positionChange()`.
    pub fn position_change(&self) -> Offset {
        if self.is_consumed() { Offset::ZERO } else { self.position_change_ignore_consumed() }
    }

    pub fn position_change_ignore_consumed(&self) -> Offset {
        self.position - self.previous_position
    }

    pub fn position_changed(&self) -> bool {
        self.position_change() != Offset::ZERO
    }

    pub fn position_changed_ignore_consumed(&self) -> bool {
        self.position_change_ignore_consumed() != Offset::ZERO
    }

    /// Whether it's outside a node of `size`, as `isOutOfBounds(size)`.
    pub fn is_out_of_bounds(&self, size: IntSize) -> bool {
        self.is_out_of_bounds_with(size, Size::new(0.0, 0.0))
    }

    /// Whether it's outside a node of `size`, a touch only if it's `padding` further out, as
    /// `isOutOfBounds(size, extendedTouchPadding)`.
    pub fn is_out_of_bounds_with(&self, size: IntSize, padding: Size) -> bool {
        let is_touch = self.pointer_type == PointerType::Touch;
        let (extra_x, extra_y) =
            if is_touch { (padding.width, padding.height) } else { (0.0, 0.0) };
        let Offset { x, y } = self.position;
        x < -extra_x
            || x > size.width as f32 + extra_x
            || y < -extra_y
            || y > size.height as f32 + extra_y
    }
}

impl std::fmt::Debug for PointerInputChange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PointerInputChange")
            .field("id", &self.id)
            .field("uptime_millis", &self.uptime_millis)
            .field("position", &self.position)
            .field("pressed", &self.pressed)
            .field("previous_position", &self.previous_position)
            .field("previous_pressed", &self.previous_pressed)
            .field("is_consumed", &self.is_consumed())
            .finish()
    }
}

/// One platform event as a node hears it, as Compose's `PointerEvent`: the change of each pointer
/// that hit the node.
#[derive(Clone, Debug)]
pub struct PointerEvents {
    pub changes: Vec<PointerInputChange>,
    /// The buttons during it.
    pub buttons: PointerButtons,
    /// The modifier keys during it.
    pub keyboard_modifiers: PointerKeyboardModifiers,
    /// The reason it was sent, as `type`: of the whole event, whichever pointer it's about.
    pub kind: PointerEventType,
}

impl PointerEvents {
    /// An event of `changes`, as `PointerEvent(changes)`: its reason found in them, with no buttons
    /// or keys.
    pub fn new(changes: Vec<PointerInputChange>) -> PointerEvents {
        let kind = kind_of(&changes);
        PointerEvents {
            changes,
            buttons: PointerButtons::NONE,
            keyboard_modifiers: PointerKeyboardModifiers::default(),
            kind,
        }
    }
}

impl PointerEvents {
    /// Whether it's every pointer coming down, unconsumed if `require_unconsumed`, as
    /// `isChangedToDown`; a mouse's only with its primary button if `primary_only`, a press saying no
    /// button having only that.
    pub fn is_changed_to_down(&self, require_unconsumed: bool, primary_only: bool) -> bool {
        let only_mice = self.changes.iter().all(|change| change.pointer_type == PointerType::Mouse);
        if primary_only
            && only_mice
            && !self.buttons.is_primary_pressed()
            && self.buttons.are_any_pressed()
        {
            return false;
        }
        self.changes.iter().all(|change| {
            if require_unconsumed {
                change.changed_to_down()
            } else {
                change.changed_to_down_ignore_consumed()
            }
        })
    }
}

/// The first of a pointer coming up or down, else a move, as `calculatePointerEventType`.
fn kind_of(changes: &[PointerInputChange]) -> PointerEventType {
    for change in changes {
        if change.changed_to_up_ignore_consumed() {
            return PointerEventType::Release;
        }
        if change.changed_to_down_ignore_consumed() {
            return PointerEventType::Press;
        }
    }
    PointerEventType::Move
}
