//! Input injection, as Compose's `performTouchInput`, `performMouseInput` and
//! `performKeyInput` with their `InjectionScope`s: gestures in the node's coordinates, sent
//! through the host as real input events, each at its time on the test clock (the clock moves
//! as the events' times do, as Compose's `InputDispatcher` moves its main clock).

use std::collections::BTreeMap;
use std::ops::Deref;
use std::time::Duration;

use compose_ui::{
    Density, Dp, IntSize, Key, KeyEvent, KeyEventType, KeyModifiers, Offset, PointerButtons,
    PointerEvent, PointerEventType, PointerId, PointerKeyboardModifiers, PointerType, Rect,
    ViewConfiguration, platform::NodeId,
};

use crate::interaction::SemanticsNodeInteraction;
use crate::rule::ComposeTestRule;

/// The time between two injected events, as Compose's `InputDispatcher.eventPeriodMillis`.
pub(crate) const EVENT_PERIOD_MILLIS: u64 = 16;
/// A held key repeats after this long, then every [`SUBSEQUENT_REPEAT_DELAY`], as Compose's.
const INITIAL_REPEAT_DELAY: u64 = 500;
const SUBSEQUENT_REPEAT_DELAY: u64 = 50;
/// A key press's default duration and the pause between presses, as Compose's.
const DEFAULT_KEY_PRESS_MILLIS: u64 = 50;
/// A mouse click's press duration, as Compose's `SingleClickDelayMillis`.
const SINGLE_CLICK_DELAY_MILLIS: u64 = 60;
/// A mouse gesture's default duration, as Compose's.
const DEFAULT_MOUSE_GESTURE_MILLIS: u64 = 300;
/// The swipes' default duration, as Compose's.
pub(crate) const DEFAULT_SWIPE_MILLIS: u64 = 200;
/// A wheel's step in pixels per unit of `scroll`'s delta, as Android's (64 dp).
const SCROLL_DP_PER_UNIT: f32 = 64.0;
/// How far back a velocity is measured, as Compose's `HorizonMilliseconds`.
const HORIZON_MILLIS: u64 = 100;

/// The double tap's default delay, halfway between its least and most time, as Compose's.
pub(crate) fn default_double_tap_delay_millis(configuration: &ViewConfiguration) -> u64 {
    (configuration.double_tap_min_time_millis + configuration.double_tap_timeout_millis) / 2
}

/// One finger's swipe of `multi_touch_swipe_with_velocity`, as Compose's `SwipeInfo`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SwipeInfo {
    pub start: Offset,
    pub end: Offset,
    /// Its velocity at the end, in px/s, 0 or more.
    pub end_velocity: f32,
}

/// A mouse button, as Compose's `MouseButton`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum MouseButton {
    #[default]
    Primary,
    Secondary,
    Tertiary,
}

impl MouseButton {
    /// Its id, as `buttonId`: its bit among an event's buttons (Android's `MotionEvent`
    /// `BUTTON_*`), which the injection's errors name (D2080).
    pub fn button_id(self) -> i32 {
        self.bit().0
    }

    /// Its bit among an event's buttons.
    fn bit(self) -> PointerButtons {
        match self {
            MouseButton::Primary => PointerButtons::PRIMARY,
            MouseButton::Secondary => PointerButtons::SECONDARY,
            MouseButton::Tertiary => PointerButtons::TERTIARY,
        }
    }
}

/// A mouse's scroll wheel, as Compose's `ScrollWheel`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum ScrollWheel {
    #[default]
    Vertical,
    Horizontal,
}

/// What moves the cursor, as Compose's `InputSource`: both are the cursor's pointer (pointer 0,
/// a mouse to the tree), told apart in what injection says.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CursorSource {
    Mouse,
    Trackpad,
}

impl CursorSource {
    fn name(self) -> &'static str {
        match self {
            CursorSource::Mouse => "mouse",
            CursorSource::Trackpad => "trackpad",
        }
    }
}

/// What injection keeps across blocks: the touches down, the mouse, the keys held, as
/// Compose's `InputDispatcherState`. Positions are in the root.
#[derive(Default)]
pub(crate) struct InputState {
    /// The fingers down, by pointer id, and whether one moved since the last event.
    touches: BTreeMap<u64, Offset>,
    touch_updated: bool,
    /// The cursor, which the mouse and a trackpad move, as Compose's `CursorInputState`.
    mouse: Offset,
    mouse_entered: bool,
    mouse_buttons: PointerButtons,
    /// Which of them moved it last, as `currentInputSource`.
    cursor_source: Option<CursorSource>,
    /// A trackpad's pan in progress and how far it panned, its pinch and how much it scaled.
    pan: Option<Offset>,
    scale: Option<f32>,
    /// An indirect pointer gesture in progress.
    pub(crate) indirect: Option<crate::indirect::IndirectGesture>,
    keys: Vec<Key>,
    /// When the last key went down, the key that repeats, and when it last repeated (ms of the
    /// test clock).
    key_down_time: u64,
    repeat_key: Option<Key>,
    last_repeat_time: u64,
    /// The lock keys' states, each a step of Compose's `LockKeyState` cycle: 0 up and off,
    /// 1 down and on, 2 up and on, 3 down (and still on until its release).
    caps_lock: u8,
    num_lock: u8,
    scroll_lock: u8,
}

/// Touch pointers' ids: the first finger is 0, as Compose's; offset so the mouse (pointer 0 in
/// compose-rs's hosts) and fingers don't share a capture.
fn touch_pointer(id: u64) -> PointerId {
    PointerId(id + 1)
}

/// Ends a touch gesture in progress: an indirect pointer's event cancels it, as Compose's
/// dispatcher.
pub(crate) fn cancel_touch_gesture(rule: &mut ComposeTestRule) {
    let ids: Vec<u64> = std::mem::take(&mut rule.input.touches).into_keys().collect();
    rule.input.touch_updated = false;
    for id in ids {
        rule.host.with_tree(|tree| tree.cancel_pointer(touch_pointer(id)));
    }
}

/// The geometry and timing every injection scope shares, as Compose's `InjectionScope`: the
/// node's visible bounds (positions are relative to their top left), the density, the
/// `ViewConfiguration`, and the event time.
pub struct InjectionScope<'r> {
    rule: &'r mut ComposeTestRule,
    /// The node's visible bounds' top left in the root.
    origin: Offset,
    visible_size: IntSize,
    root: Rect,
    density: Density,
    view_configuration: ViewConfiguration,
}

impl<'r> InjectionScope<'r> {
    fn new(
        rule: &'r mut ComposeTestRule,
        node: NodeId,
        bounds: Rect,
        root: Rect,
    ) -> InjectionScope<'r> {
        let (density, view_configuration) =
            rule.host.with_tree(|tree| (tree.density_of(node), tree.view_configuration_of(node)));
        InjectionScope {
            rule,
            origin: Offset::new(bounds.left, bounds.top),
            visible_size: IntSize::new(
                compose_ui::platform::round_to_int(bounds.width()),
                compose_ui::platform::round_to_int(bounds.height()),
            ),
            root,
            density,
            view_configuration,
        }
    }

    /// The time between two events by default, as `eventPeriodMillis` (16 ms).
    pub fn event_period_millis(&self) -> u64 {
        EVENT_PERIOD_MILLIS
    }

    /// The node's density, as the scope's `Density` (`semanticsNode.layoutInfo.density`).
    pub fn density(&self) -> Density {
        self.density
    }

    /// The node's, as `viewConfiguration` (`semanticsNode.layoutInfo.viewConfiguration`), which
    /// its gesture detectors use.
    pub fn view_configuration(&self) -> ViewConfiguration {
        self.view_configuration
    }

    /// The node's visible size (its bounds clipped by the clips around it), as `visibleSize`.
    pub fn visible_size(&self) -> IntSize {
        self.visible_size
    }

    pub fn width(&self) -> i32 {
        self.visible_size.width
    }

    pub fn height(&self) -> i32 {
        self.visible_size.height
    }

    pub fn left(&self) -> f32 {
        0.0
    }

    pub fn top(&self) -> f32 {
        0.0
    }

    pub fn center_x(&self) -> f32 {
        self.width() as f32 / 2.0
    }

    pub fn center_y(&self) -> f32 {
        self.height() as f32 / 2.0
    }

    /// The last pixel column inside, as `right` (width − 1).
    pub fn right(&self) -> f32 {
        if self.width() == 0 { 0.0 } else { self.width() as f32 - 1.0 }
    }

    /// The last pixel row inside, as `bottom`.
    pub fn bottom(&self) -> f32 {
        if self.height() == 0 { 0.0 } else { self.height() as f32 - 1.0 }
    }

    pub fn top_left(&self) -> Offset {
        Offset::new(self.left(), self.top())
    }

    pub fn top_center(&self) -> Offset {
        Offset::new(self.center_x(), self.top())
    }

    pub fn top_right(&self) -> Offset {
        Offset::new(self.right(), self.top())
    }

    pub fn center_left(&self) -> Offset {
        Offset::new(self.left(), self.center_y())
    }

    pub fn center(&self) -> Offset {
        Offset::new(self.center_x(), self.center_y())
    }

    pub fn center_right(&self) -> Offset {
        Offset::new(self.right(), self.center_y())
    }

    pub fn bottom_left(&self) -> Offset {
        Offset::new(self.left(), self.bottom())
    }

    pub fn bottom_center(&self) -> Offset {
        Offset::new(self.center_x(), self.bottom())
    }

    pub fn bottom_right(&self) -> Offset {
        Offset::new(self.right(), self.bottom())
    }

    /// A position as fractions of the size, as `percentOffset`.
    pub fn percent_offset(&self, x: f32, y: f32) -> Offset {
        Offset::new(x * self.width() as f32, y * self.height() as f32)
    }

    /// Moves the event time (and the test clock) on by `duration_millis`, as
    /// `advanceEventTime`: frames and tasks due on the way run; a held key repeats.
    pub fn advance_event_time(&mut self, duration_millis: u64) {
        let end = self.rule.current_time_millis() + duration_millis;
        self.send_repeat_keys(end);
        let now = self.rule.current_time_millis();
        if end > now {
            self.rule.advance_time_by_ignoring_frame_duration(Duration::from_millis(end - now));
        }
    }

    fn to_root(&self, position: Offset) -> Offset {
        position + self.origin
    }

    fn to_local(&self, position: Offset) -> Offset {
        position - self.origin
    }

    fn within_root(&self, position: Offset) -> bool {
        self.root.contains(position)
    }

    fn now_nanos(&self) -> u64 {
        compose_runtime::platform::now_nanos()
    }

    fn send(&mut self, event: PointerEvent) {
        self.send_together(&[event]);
    }

    /// The changes of one event, at its time, with the keys held.
    fn send_together(&mut self, events: &[PointerEvent]) {
        let (time, modifiers) = (self.now_nanos(), self.pointer_modifiers());
        let events: Vec<PointerEvent> = events
            .iter()
            .map(|event| event.at_time(time).with_keyboard_modifiers(modifiers))
            .collect();
        self.rule.host.send_pointers(&events);
        self.rule.run_current();
    }

    /// The keys a pointer event tells: the modifiers held and the locks on, as Compose's
    /// dispatcher's meta state (D2076).
    fn pointer_modifiers(&self) -> PointerKeyboardModifiers {
        // Android's `META_CAPS_LOCK_ON`, `META_NUM_LOCK_ON` and `META_SCROLL_LOCK_ON`, the bits
        // `PointerKeyboardModifiers` holds.
        let input = &self.rule.input;
        let lock = |state: u8, bit: i32| if state > 0 { bit } else { 0 };
        let locks = lock(input.caps_lock, 0x10_0000)
            | lock(input.num_lock, 0x20_0000)
            | lock(input.scroll_lock, 0x40_0000);
        PointerKeyboardModifiers(PointerKeyboardModifiers::from(self.modifiers()).0 | locks)
    }

    fn modifiers(&self) -> KeyModifiers {
        let down = |keys: [Key; 2]| keys.iter().any(|key| self.rule.input.keys.contains(key));
        KeyModifiers {
            shift: down([Key::ShiftLeft, Key::ShiftRight]),
            ctrl: down([Key::CtrlLeft, Key::CtrlRight]),
            alt: down([Key::AltLeft, Key::AltRight]),
            meta: down([Key::MetaLeft, Key::MetaRight]),
        }
    }

    fn send_key(&mut self, key: Key, kind: KeyEventType) {
        let modifiers = self.modifiers();
        self.rule.host.send_key(KeyEvent { key, kind, modifiers, from_soft_keyboard: false });
        self.rule.run_current();
    }

    /// The repeats of the held key due before `end` (ms), each at its time, as Compose's
    /// `sendRepeatKeysIfNeeded`.
    fn send_repeat_keys(&mut self, end: u64) {
        let Some(key) = self.rule.input.repeat_key else { return };
        let down_time = self.rule.input.key_down_time;
        if end.saturating_sub(down_time) < INITIAL_REPEAT_DELAY {
            return;
        }
        let mut next = if self.rule.input.last_repeat_time <= down_time {
            down_time + INITIAL_REPEAT_DELAY
        } else {
            self.rule.input.last_repeat_time + SUBSEQUENT_REPEAT_DELAY
        };
        while next <= end {
            let now = self.rule.current_time_millis();
            if next > now {
                self.rule
                    .advance_time_by_ignoring_frame_duration(Duration::from_millis(next - now));
            }
            self.rule.input.last_repeat_time = next;
            self.send_key(key, KeyEventType::KeyDown);
            next += SUBSEQUENT_REPEAT_DELAY;
        }
    }
}

// ---- Touch -------------------------------------------------------------------------------------

/// Touch gestures on a node, as Compose's `TouchInjectionScope`: fingers by pointer id (0 the
/// first), positions in the node's coordinates.
pub struct TouchInjectionScope<'r>(InjectionScope<'r>);

impl<'r> Deref for TouchInjectionScope<'r> {
    type Target = InjectionScope<'r>;
    fn deref(&self) -> &InjectionScope<'r> {
        &self.0
    }
}

impl TouchInjectionScope<'_> {
    fn input(&mut self) -> &mut InputState {
        &mut self.0.rule.input
    }

    fn touch_event(&mut self, event: PointerEvent, id: u64) {
        self.0.send(event.with_pointer(touch_pointer(id), PointerType::Touch));
    }

    /// Where pointer `id` is, in the node's coordinates, if it's down, as `currentPosition`.
    pub fn current_position(&self, id: u64) -> Option<Offset> {
        self.0.rule.input.touches.get(&id).map(|position| self.0.to_local(*position))
    }

    /// Puts the first finger down at `position`, as `down(position)`.
    #[track_caller]
    pub fn down(&mut self, position: Offset) {
        self.down_pointer(0, position);
    }

    /// Puts finger `id` down at `position`, as `down(pointerId, position)`: a move of the other
    /// fingers updated since goes first.
    #[track_caller]
    pub fn down_pointer(&mut self, id: u64, position: Offset) {
        assert!(
            !self.input().touches.contains_key(&id),
            "compose-test: Cannot send DOWN event, a gesture is already in progress for pointer {id}"
        );
        crate::indirect::cancel_indirect_gesture(self.0.rule);
        if self.input().mouse_buttons.are_any_pressed() {
            self.0.rule.input.mouse_buttons = PointerButtons::NONE;
            self.0.rule.host.with_tree(|tree| tree.cancel_pointer(PointerId(0)));
        } else if self.input().mouse_entered {
            self.0.rule.input.mouse_entered = false;
            self.0.send_cursor_left();
        }
        if self.input().touch_updated {
            self.send_move();
        }
        let root = self.0.to_root(position);
        self.input().touches.insert(id, root);
        self.touch_event(PointerEvent::press(root), id);
    }

    /// Moves finger `id` to `position` without sending an event, as `updatePointerTo`: the next
    /// move (or down) sends it.
    #[track_caller]
    pub fn update_pointer_to(&mut self, id: u64, position: Offset) {
        let root = self.0.to_root(position);
        assert!(
            !self.input().touches.is_empty(),
            "compose-test: Cannot move pointers, no gesture is in progress"
        );
        let touch = self.input().touches.get_mut(&id);
        let Some(touch) = touch else {
            panic!(
                "compose-test: Cannot move pointer {id}, it is not active in the current gesture"
            )
        };
        *touch = root;
        self.input().touch_updated = true;
    }

    /// Moves finger `id` by `delta` without sending an event, as `updatePointerBy`.
    #[track_caller]
    pub fn update_pointer_by(&mut self, id: u64, delta: Offset) {
        let current = self.current_position(id).unwrap_or(Offset::ZERO);
        self.update_pointer_to(id, current + delta);
    }

    /// Sends a move of every finger down, an event period after the last event, as `move()`.
    #[track_caller]
    pub fn move_pointers(&mut self) {
        self.move_pointers_after(EVENT_PERIOD_MILLIS);
    }

    /// The same, `delay_millis` after the last event, as `move(delayMillis)`.
    #[track_caller]
    pub fn move_pointers_after(&mut self, delay_millis: u64) {
        assert!(
            !self.input().touches.is_empty(),
            "compose-test: Cannot send MOVE event, no gesture is in progress"
        );
        self.0.advance_event_time(delay_millis);
        self.send_move();
    }

    /// One move event of every finger down.
    fn send_move(&mut self) {
        self.input().touch_updated = false;
        let touches: Vec<PointerEvent> = self
            .input()
            .touches
            .iter()
            .map(|(id, position)| {
                PointerEvent::moved(*position, true)
                    .with_pointer(touch_pointer(*id), PointerType::Touch)
            })
            .collect();
        self.0.send_together(&touches);
    }

    /// Moves the first finger to `position`, an event period on, as `moveTo(position)`.
    #[track_caller]
    pub fn move_to(&mut self, position: Offset) {
        self.move_pointer_to(0, position, EVENT_PERIOD_MILLIS);
    }

    /// Moves finger `id` to `position`, `delay_millis` on, as `moveTo(pointerId, position,
    /// delayMillis)`.
    #[track_caller]
    pub fn move_pointer_to(&mut self, id: u64, position: Offset, delay_millis: u64) {
        self.update_pointer_to(id, position);
        self.move_pointers_after(delay_millis);
    }

    /// Moves the first finger by `delta`, an event period on, as `moveBy(delta)`.
    #[track_caller]
    pub fn move_by(&mut self, delta: Offset) {
        self.move_pointer_by(0, delta, EVENT_PERIOD_MILLIS);
    }

    /// Moves finger `id` by `delta`, `delay_millis` on, as `moveBy(pointerId, delta,
    /// delayMillis)`.
    #[track_caller]
    pub fn move_pointer_by(&mut self, id: u64, delta: Offset, delay_millis: u64) {
        self.update_pointer_by(id, delta);
        self.move_pointers_after(delay_millis);
    }

    /// Lifts the first finger, as `up()`.
    #[track_caller]
    pub fn up(&mut self) {
        self.up_pointer(0);
    }

    /// Lifts finger `id`, as `up(pointerId)`.
    #[track_caller]
    pub fn up_pointer(&mut self, id: u64) {
        assert!(
            !self.input().touches.is_empty(),
            "compose-test: Cannot send UP event, no gesture is in progress"
        );
        let Some(position) = self.input().touches.get(&id).copied() else {
            panic!(
                "compose-test: Cannot send UP event for pointer {id}, it is not active in the \
                 current gesture"
            )
        };
        // The fingers updated since the last event move in the release's own event, not a move
        // first, as Compose's `enqueueTouchUp` (D2075).
        let mut changes: Vec<PointerEvent> = Vec::new();
        if std::mem::take(&mut self.input().touch_updated) {
            changes.extend(self.input().touches.iter().filter(|(other, _)| **other != id).map(
                |(other, at)| {
                    PointerEvent::moved(*at, true)
                        .with_pointer(touch_pointer(*other), PointerType::Touch)
                },
            ));
        }
        changes.push(
            PointerEvent::release(position).with_pointer(touch_pointer(id), PointerType::Touch),
        );
        self.0.send_together(&changes);
        self.input().touches.remove(&id);
    }

    /// Cancels the gesture, an event period on, as `cancel()`.
    #[track_caller]
    pub fn cancel(&mut self) {
        self.cancel_after(EVENT_PERIOD_MILLIS);
    }

    /// Cancels the gesture, `delay_millis` on, as `cancel(delayMillis)`.
    #[track_caller]
    pub fn cancel_after(&mut self, delay_millis: u64) {
        assert!(
            !self.input().touches.is_empty(),
            "compose-test: Cannot send CANCEL event, no gesture is in progress"
        );
        self.0.advance_event_time(delay_millis);
        let ids: Vec<u64> = std::mem::take(&mut self.input().touches).into_keys().collect();
        self.input().touch_updated = false;
        for id in ids {
            self.0.rule.host.with_tree(|tree| tree.cancel_pointer(touch_pointer(id)));
        }
        self.0.rule.run_current();
    }

    // ---- Gestures -------------------------------------------------------------------------

    /// A tap at the center: down, a move an event period later, up, as `click()`.
    pub fn click(&mut self) {
        let center = self.center();
        self.click_at(center);
    }

    /// A tap at `position`, as `click(position)`.
    pub fn click_at(&mut self, position: Offset) {
        self.down(position);
        self.move_pointers();
        self.up();
    }

    /// A long press at the center, 100 ms past the long-press timeout, as `longClick()`.
    pub fn long_click(&mut self) {
        let center = self.center();
        let duration = self.view_configuration().long_press_timeout_millis + 100;
        self.long_click_at(center, duration);
    }

    /// A long press at `position` held `duration_millis` (at least the long-press timeout), as
    /// `longClick(position, durationMillis)`.
    #[track_caller]
    pub fn long_click_at(&mut self, position: Offset, duration_millis: u64) {
        let timeout = self.view_configuration().long_press_timeout_millis;
        assert!(
            duration_millis >= timeout,
            "compose-test: Long click must have a duration of at least {timeout}ms"
        );
        self.swipe(position, position, duration_millis);
    }

    /// Two taps at the center, halfway through the double-tap window apart, as
    /// `doubleClick()`.
    pub fn double_click(&mut self) {
        let center = self.center();
        let delay = default_double_tap_delay_millis(&self.view_configuration());
        self.double_click_at(center, delay);
    }

    /// Two taps at `position`, `delay_millis` apart, as `doubleClick(position, delayMillis)`.
    #[track_caller]
    pub fn double_click_at(&mut self, position: Offset, delay_millis: u64) {
        let configuration = self.view_configuration();
        assert!(
            delay_millis >= configuration.double_tap_min_time_millis,
            "compose-test: Time between clicks in double click must be at least {}ms",
            configuration.double_tap_min_time_millis
        );
        assert!(
            delay_millis < configuration.double_tap_timeout_millis,
            "compose-test: Time between clicks in double click must be smaller than {}ms",
            configuration.double_tap_timeout_millis
        );
        self.click_at(position);
        self.0.advance_event_time(delay_millis);
        self.click_at(position);
    }

    /// A straight swipe from `start` to `end` over `duration_millis`, a move about every event
    /// period, as `swipe(start, end, durationMillis)`.
    pub fn swipe(&mut self, start: Offset, end: Offset, duration_millis: u64) {
        let duration = duration_millis as f32;
        self.swipe_along(&|time| lerp(start, end, time as f32 / duration), duration_millis, &[]);
    }

    /// A swipe along `curve` (the position at each time, in ms), through `key_times`, as
    /// `swipe(curve, durationMillis, keyTimes)`.
    pub fn swipe_along(
        &mut self,
        curve: &dyn Fn(u64) -> Offset,
        duration_millis: u64,
        key_times: &[u64],
    ) {
        self.multi_touch_swipe(&[curve], duration_millis, key_times);
    }

    /// A swipe of a finger per curve, as `multiTouchSwipe`: the fingers go down at their
    /// curves' starts, move together about every event period (and at each key time), and
    /// lift at the end.
    #[track_caller]
    pub fn multi_touch_swipe(
        &mut self,
        curves: &[&dyn Fn(u64) -> Offset],
        duration_millis: u64,
        key_times: &[u64],
    ) {
        assert!(duration_millis >= 1, "compose-test: duration must be at least 1 millisecond");
        assert!(
            key_times.iter().all(|time| *time <= duration_millis),
            "compose-test: keyTimes contains timestamps out of range [0..{duration_millis}]: \
             {key_times:?}"
        );
        assert!(
            key_times.windows(2).all(|pair| pair[0] <= pair[1]),
            "compose-test: keyTimes must be sorted: {key_times:?}"
        );
        for (index, curve) in curves.iter().enumerate() {
            self.down_pointer(index as u64, curve(0));
        }
        let mut time = 0;
        let mut key = 0;
        while time < duration_millis {
            while key < key_times.len() && key_times[key] <= time {
                key += 1;
            }
            let next = key_times.get(key).copied().unwrap_or(duration_millis);
            self.swipe_segment(curves, time, next);
            time = next;
        }
        for index in 0..curves.len() {
            self.up_pointer(index as u64);
        }
    }

    fn swipe_segment(&mut self, curves: &[&dyn Fn(u64) -> Offset], start: u64, end: u64) {
        let steps = (((end - start) as f32 / EVENT_PERIOD_MILLIS as f32).round() as u64).max(1);
        let mut previous = start;
        for step in 1..=steps {
            let progress = step as f32 / steps as f32;
            let time = lerp_time(start, end, progress);
            for (index, curve) in curves.iter().enumerate() {
                self.update_pointer_to(index as u64, curve(time));
            }
            self.move_pointers_after(time - previous);
            previous = time;
        }
    }

    /// Two fingers moving from `start0` and `start1` to `end0` and `end1`, as `pinch`.
    pub fn pinch(
        &mut self,
        start0: Offset,
        end0: Offset,
        start1: Offset,
        end1: Offset,
        duration_millis: u64,
    ) {
        let duration = duration_millis as f32;
        let first = move |time: u64| lerp(start0, end0, time as f32 / duration);
        let second = move |time: u64| lerp(start1, end1, time as f32 / duration);
        self.multi_touch_swipe(&[&first, &second], duration_millis, &[]);
    }

    /// A swipe ending at `end_velocity` px/s, as `swipeWithVelocity`, over a default duration
    /// computed from it (`None`), along Compose's portable velocity path (its
    /// `LegacyVelocityPathFinder`): still, then a quadratic over the last 100 ms at most.
    #[track_caller]
    pub fn swipe_with_velocity(
        &mut self,
        start: Offset,
        end: Offset,
        end_velocity: f32,
        duration_millis: Option<u64>,
    ) {
        assert!(
            end_velocity >= 0.0,
            "compose-test: Velocity cannot be {end_velocity}, it must be positive"
        );
        let duration =
            duration_millis.unwrap_or_else(|| default_velocity_duration(start, end, end_velocity));
        let minimum = (2.5 * EVENT_PERIOD_MILLIS as f32).ceil() as u64;
        assert!(
            duration >= minimum,
            "compose-test: Duration must be at least {minimum}ms because velocity requires at \
             least 3 input events"
        );
        let path = VelocityPath::new(start, end, end_velocity, duration);
        self.swipe_along(&|time| path.offset_at(time), duration, &[]);
    }

    /// Swipes of a finger each (pointers 0, 1, …), together, each ending at its velocity, as
    /// `multiTouchSwipeWithVelocity`: each along `swipe_with_velocity`'s path, over
    /// `duration_millis` (`None`: the longest of their default durations), through `key_times`.
    #[track_caller]
    pub fn multi_touch_swipe_with_velocity(
        &mut self,
        swipes: &[SwipeInfo],
        duration_millis: Option<u64>,
        key_times: &[u64],
    ) {
        assert!(!swipes.is_empty(), "compose-test: Swipes list cannot be empty");
        for swipe in swipes {
            assert!(
                swipe.end_velocity >= 0.0,
                "compose-test: Velocity cannot be {}, it must be positive",
                swipe.end_velocity
            );
        }
        let duration = duration_millis.unwrap_or_else(|| {
            swipes
                .iter()
                .map(|swipe| default_velocity_duration(swipe.start, swipe.end, swipe.end_velocity))
                .max()
                .unwrap_or(DEFAULT_SWIPE_MILLIS)
        });
        let minimum = (2.5 * EVENT_PERIOD_MILLIS as f32).ceil() as u64;
        assert!(
            duration >= minimum,
            "compose-test: Duration must be at least {minimum}ms because velocity requires at \
             least 3 input events"
        );
        let paths: Vec<VelocityPath> = swipes
            .iter()
            .map(|swipe| VelocityPath::new(swipe.start, swipe.end, swipe.end_velocity, duration))
            .collect();
        let curves: Vec<Box<dyn Fn(u64) -> Offset + '_>> = paths
            .iter()
            .map(|path| Box::new(move |time| path.offset_at(time)) as Box<dyn Fn(u64) -> Offset>)
            .collect();
        let curves: Vec<&dyn Fn(u64) -> Offset> = curves.iter().map(|curve| &**curve).collect();
        self.multi_touch_swipe(&curves, duration, key_times);
    }

    /// A swipe up the node's middle, bottom to top, as `swipeUp()`.
    pub fn swipe_up(&mut self) {
        let (bottom, top) = (self.bottom(), self.top());
        self.swipe_up_with(bottom, top, DEFAULT_SWIPE_MILLIS);
    }

    /// As `swipeUp(startY, endY, durationMillis)`.
    #[track_caller]
    pub fn swipe_up_with(&mut self, start_y: f32, end_y: f32, duration_millis: u64) {
        assert!(
            start_y >= end_y,
            "compose-test: startY={start_y} needs to be greater than or equal to endY={end_y}"
        );
        let x = self.center_x();
        self.swipe(Offset::new(x, start_y), Offset::new(x, end_y), duration_millis);
    }

    /// A swipe down the node's middle, as `swipeDown()`.
    pub fn swipe_down(&mut self) {
        let (top, bottom) = (self.top(), self.bottom());
        self.swipe_down_with(top, bottom, DEFAULT_SWIPE_MILLIS);
    }

    /// As `swipeDown(startY, endY, durationMillis)`.
    #[track_caller]
    pub fn swipe_down_with(&mut self, start_y: f32, end_y: f32, duration_millis: u64) {
        assert!(
            start_y <= end_y,
            "compose-test: startY={start_y} needs to be less than or equal to endY={end_y}"
        );
        let x = self.center_x();
        self.swipe(Offset::new(x, start_y), Offset::new(x, end_y), duration_millis);
    }

    /// A swipe left across the node's middle, as `swipeLeft()`.
    pub fn swipe_left(&mut self) {
        let (right, left) = (self.right(), self.left());
        self.swipe_left_with(right, left, DEFAULT_SWIPE_MILLIS);
    }

    /// As `swipeLeft(startX, endX, durationMillis)`.
    #[track_caller]
    pub fn swipe_left_with(&mut self, start_x: f32, end_x: f32, duration_millis: u64) {
        assert!(
            start_x >= end_x,
            "compose-test: startX={start_x} needs to be greater than or equal to endX={end_x}"
        );
        let y = self.center_y();
        self.swipe(Offset::new(start_x, y), Offset::new(end_x, y), duration_millis);
    }

    /// A swipe right across the node's middle, as `swipeRight()`.
    pub fn swipe_right(&mut self) {
        let (left, right) = (self.left(), self.right());
        self.swipe_right_with(left, right, DEFAULT_SWIPE_MILLIS);
    }

    /// As `swipeRight(startX, endX, durationMillis)`.
    #[track_caller]
    pub fn swipe_right_with(&mut self, start_x: f32, end_x: f32, duration_millis: u64) {
        assert!(
            start_x <= end_x,
            "compose-test: startX={start_x} needs to be less than or equal to endX={end_x}"
        );
        let y = self.center_y();
        self.swipe(Offset::new(start_x, y), Offset::new(end_x, y), duration_millis);
    }

    /// As [`InjectionScope::advance_event_time`].
    pub fn advance_event_time(&mut self, duration_millis: u64) {
        self.0.advance_event_time(duration_millis);
    }
}

pub(crate) fn lerp(start: Offset, end: Offset, fraction: f32) -> Offset {
    Offset::new(start.x + (end.x - start.x) * fraction, start.y + (end.y - start.y) * fraction)
}

/// Kotlin's `lerp(Long, Long, Float)`: rounded.
pub(crate) fn lerp_time(start: u64, end: u64, fraction: f32) -> u64 {
    start + ((end - start) as f64 * fraction as f64).round() as u64
}

/// The default duration of a swipe ending at `velocity`, as Compose's
/// `VelocityPathFinder.calculateDefaultDuration`.
#[track_caller]
pub(crate) fn default_velocity_duration(start: Offset, end: Offset, velocity: f32) -> u64 {
    assert!(
        start != end || velocity == 0.0,
        "compose-test: When start == end; velocity cannot be {velocity}, it must be 0f"
    );
    let distance = distance(start, end);
    let duration = 2.0 / velocity * distance * 1000.0;
    if duration.is_nan() || duration > HORIZON_MILLIS as f32 {
        return DEFAULT_SWIPE_MILLIS;
    }
    let minimum = (2.5 * EVENT_PERIOD_MILLIS as f32).ceil() as u64;
    let duration = duration.floor() as u64;
    assert!(
        duration >= minimum,
        "compose-test: Unable to generate a swipe gesture between {start:?} and {end:?} that \
         ends with velocity of {velocity} px/s, without going outside of the range [start..end]"
    );
    duration
}

fn distance(a: Offset, b: Offset) -> f32 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

/// A path from `start` to `end` ending at a velocity, as Compose's `LegacyVelocityPathFinder`:
/// still, then on each axis a parabola reaching the end at the velocity along the direction.
pub(crate) struct VelocityPath {
    start: Offset,
    end: Offset,
    duration: u64,
    vx: f64,
    vy: f64,
}

impl VelocityPath {
    pub(crate) fn new(start: Offset, end: Offset, velocity: f32, duration: u64) -> VelocityPath {
        let theta = ((end.y - start.y) as f64).atan2((end.x - start.x) as f64);
        VelocityPath {
            start,
            end,
            duration,
            vx: theta.cos() * velocity as f64 / 1000.0,
            vy: theta.sin() * velocity as f64 / 1000.0,
        }
    }

    pub(crate) fn offset_at(&self, time: u64) -> Offset {
        Offset::new(
            self.axis(self.vx, self.start.x, self.end.x, time),
            self.axis(self.vy, self.start.y, self.end.y, time),
        )
    }

    #[track_caller]
    fn axis(&self, velocity: f64, start: f32, end: f32, time: u64) -> f32 {
        let total = self.duration as f64;
        let d = if start == end { total } else { total.min(2.0 / velocity * (end - start) as f64) };
        let a = (start as f64 + velocity * d - end as f64) / (d * d);
        assert!(
            d >= total.min(HORIZON_MILLIS as f64),
            "compose-test: Unable to generate a swipe gesture between {:?} and {:?} with \
             duration {} that ends with that velocity without going outside of the range \
             [start..end]",
            self.start,
            self.end,
            self.duration
        );
        let threshold = total - d;
        let time = time as f64;
        if time < threshold {
            start
        } else {
            let t = time - total;
            (a * t * t + velocity * t + end as f64) as f32
        }
    }
}

// ---- The cursor: the mouse's and a trackpad's ------------------------------------------------------

impl InjectionScope<'_> {
    fn cursor_event(&mut self, event: PointerEvent) {
        self.send(event.with_pointer(PointerId(0), PointerType::Mouse));
    }

    /// The cursor left the window's hover, telling the modifiers held and locks on (D2131).
    fn send_cursor_left(&mut self) {
        let modifiers = self.pointer_modifiers();
        self.rule.host.send_pointer_left_with(modifiers);
    }

    /// Ends a touch gesture in progress: the cursor's events cancel it, as Compose's dispatcher.
    fn cancel_touch(&mut self) {
        if self.rule.input.touches.is_empty() {
            return;
        }
        let ids: Vec<u64> = std::mem::take(&mut self.rule.input.touches).into_keys().collect();
        self.rule.input.touch_updated = false;
        for id in ids {
            self.rule.host.with_tree(|tree| tree.cancel_pointer(touch_pointer(id)));
        }
    }

    /// The cursor is `source`'s from now: the other's buttons held are cancelled, as Compose's
    /// `enqueueMousePress` cancels a trackpad's gesture, and the other way.
    fn take_cursor(&mut self, source: CursorSource) {
        let input = &mut self.rule.input;
        if input.cursor_source.is_some_and(|it| it != source)
            && input.mouse_buttons.are_any_pressed()
        {
            input.mouse_buttons = PointerButtons::NONE;
            self.rule.host.with_tree(|tree| tree.cancel_pointer(PointerId(0)));
        }
        self.rule.input.cursor_source = Some(source);
    }

    /// The cursor's position, in the node's coordinates.
    fn cursor_position(&self) -> Offset {
        self.to_local(self.rule.input.mouse)
    }

    /// Moves the cursor to `position` `delay_millis` on, as `enqueueMouseMove` and
    /// `enqueueTrackpadMove`: it enters the window first if it was outside, and leaves it if
    /// `position` is.
    fn cursor_move_to(&mut self, source: CursorSource, position: Offset, delay_millis: u64) {
        self.advance_event_time(delay_millis);
        self.take_cursor(source);
        self.cancel_touch();
        let root = self.to_root(position);
        self.rule.input.mouse = root;
        let inside = self.within_root(root);
        // Leaving the window: the exit, then the move, as Compose's `enqueueMouseMove`.
        if !inside && self.rule.input.mouse_entered {
            self.rule.input.mouse_entered = false;
            self.send_cursor_left();
        }
        let buttons = self.rule.input.mouse_buttons;
        if inside && !buttons.are_any_pressed() {
            self.rule.input.mouse_entered = true;
        }
        self.cursor_event(PointerEvent::moved(root, false).with_buttons(buttons));
    }

    /// Presses `bit` (`label` in what's said) where the cursor is, as `enqueueMousePress` and
    /// `enqueueTrackpadPress`.
    #[track_caller]
    fn cursor_press(&mut self, source: CursorSource, bit: PointerButtons, label: &str) {
        let name = source.name();
        let held = self.rule.input.mouse_buttons;
        assert!(
            held.0 & bit.0 == 0,
            "compose-test: Cannot send {name} button down event, button {label} is already pressed"
        );
        let position = self.rule.input.mouse;
        assert!(
            self.within_root(position) || held.are_any_pressed(),
            "compose-test: Cannot start a {name} gesture outside the Compose root bounds, {name} \
             position is {position:?}"
        );
        self.take_cursor(source);
        self.cancel_touch();
        let buttons = PointerButtons(held.0 | bit.0);
        self.rule.input.mouse_buttons = buttons;
        // A press ends the hover until the release, as Compose's on Android (`exitHoverOnPress`).
        self.rule.input.mouse_entered = false;
        self.cursor_event(PointerEvent::press(position).with_buttons(buttons));
    }

    /// Releases `bit`, as `enqueueMouseRelease` and `enqueueTrackpadRelease`: the cursor hovers
    /// again.
    #[track_caller]
    fn cursor_release(&mut self, source: CursorSource, bit: PointerButtons, label: &str) {
        let name = source.name();
        let held = self.rule.input.mouse_buttons;
        assert!(
            held.0 & bit.0 != 0,
            "compose-test: Cannot send {name} button up event, button {label} is not pressed"
        );
        let buttons = PointerButtons(held.0 & !bit.0);
        self.rule.input.mouse_buttons = buttons;
        let position = self.rule.input.mouse;
        self.cursor_event(PointerEvent::release(position).with_buttons(buttons));
        if buttons.are_any_pressed() {
            return;
        }
        // Inside the window, the cursor hovers again at once: the enter and a move where it
        // is, as Compose's `enqueueMouseRelease` on Android (`exitHoverOnPress`).
        let inside = self.within_root(position);
        self.rule.input.mouse_entered = inside;
        if inside {
            self.cursor_event(PointerEvent::moved(position, false));
        } else {
            self.rule.input.cursor_source = None;
        }
    }

    /// Cancels the buttons' gesture, `delay_millis` on, as `enqueueMouseCancel` and
    /// `enqueueTrackpadCancel`.
    #[track_caller]
    fn cursor_cancel(&mut self, source: CursorSource, delay_millis: u64) {
        let name = source.name();
        self.advance_event_time(delay_millis);
        assert!(
            self.rule.input.mouse_buttons.are_any_pressed(),
            "compose-test: Cannot send {name} cancel event, no {name} buttons are pressed"
        );
        assert!(
            self.rule.input.cursor_source == Some(source),
            "compose-test: Cannot send {name} cancel event, since the current cursor input isn't \
             a {name}"
        );
        self.rule.input.mouse_buttons = PointerButtons::NONE;
        self.rule.input.cursor_source = None;
        self.rule.host.with_tree(|tree| tree.cancel_pointer(PointerId(0)));
        self.rule.run_current();
    }

    /// The cursor comes into the window at `position`, `delay_millis` on, as `enter`.
    #[track_caller]
    fn cursor_enter(&mut self, source: CursorSource, position: Offset, delay_millis: u64) {
        let name = source.name();
        assert!(
            !self.rule.input.mouse_entered,
            "compose-test: Cannot send {name} hover enter event, {name} is already hovering"
        );
        assert!(
            !self.rule.input.mouse_buttons.are_any_pressed(),
            "compose-test: Cannot send {name} hover enter event, {name} buttons are down"
        );
        self.advance_event_time(delay_millis);
        let root = self.to_root(position);
        assert!(
            self.within_root(root),
            "compose-test: Cannot send {name} hover enter event, {position:?} is out of bounds"
        );
        self.rule.input.cursor_source = Some(source);
        self.rule.input.mouse = root;
        self.rule.input.mouse_entered = true;
        self.cursor_event(PointerEvent::moved(root, false));
    }

    /// The cursor leaves the window at `position`, `delay_millis` on, as `exit`.
    #[track_caller]
    fn cursor_exit(&mut self, source: CursorSource, position: Offset, delay_millis: u64) {
        let name = source.name();
        assert!(
            self.rule.input.mouse_entered,
            "compose-test: Cannot send {name} hover exit event, {name} is not hovering"
        );
        self.advance_event_time(delay_millis);
        let root = self.to_root(position);
        self.rule.input.mouse = root;
        self.rule.input.mouse_entered = false;
        self.rule.input.cursor_source = None;
        self.send_cursor_left();
        self.rule.run_current();
    }

    /// Moves the cursor along `curve` over `duration_millis`, a move about every event period, as
    /// `animateMoveAlong`.
    #[track_caller]
    fn cursor_animate_along(
        &mut self,
        source: CursorSource,
        curve: &dyn Fn(u64) -> Offset,
        duration_millis: u64,
    ) {
        assert!(duration_millis > 0, "compose-test: Duration is 0");
        let start = curve(0);
        if start != self.cursor_position() {
            self.cursor_move_to(source, start, 0);
        }
        let steps = ((duration_millis as f32 / EVENT_PERIOD_MILLIS as f32).round() as u64).max(1);
        let mut previous = 0;
        for step in 1..=steps {
            let time = lerp_time(0, duration_millis, step as f32 / steps as f32);
            self.cursor_move_to(source, curve(time), time - previous);
            previous = time;
        }
    }
}

// ---- Mouse ---------------------------------------------------------------------------------------

/// Mouse gestures on a node, as Compose's `MouseInjectionScope`: one cursor, positions in the
/// node's coordinates.
pub struct MouseInjectionScope<'r>(InjectionScope<'r>);

impl<'r> Deref for MouseInjectionScope<'r> {
    type Target = InjectionScope<'r>;
    fn deref(&self) -> &InjectionScope<'r> {
        &self.0
    }
}

const MOUSE: CursorSource = CursorSource::Mouse;

impl MouseInjectionScope<'_> {
    fn input(&mut self) -> &mut InputState {
        &mut self.0.rule.input
    }

    fn mouse_event(&mut self, event: PointerEvent) {
        self.0.cursor_event(event);
    }

    /// The cursor's position, in the node's coordinates, as `currentPosition`.
    pub fn current_position(&self) -> Offset {
        self.0.cursor_position()
    }

    /// Moves the cursor to `position`, an event period on, as `moveTo(position)`: it enters
    /// the window first if it was outside, and leaves it if `position` is.
    pub fn move_to(&mut self, position: Offset) {
        self.move_to_after(position, EVENT_PERIOD_MILLIS);
    }

    /// The same, `delay_millis` on, as `moveTo(position, delayMillis)`.
    pub fn move_to_after(&mut self, position: Offset, delay_millis: u64) {
        self.0.cursor_move_to(MOUSE, position, delay_millis);
    }

    /// Moves the cursor by `delta`, an event period on, as `moveBy(delta)`.
    pub fn move_by(&mut self, delta: Offset) {
        self.move_by_after(delta, EVENT_PERIOD_MILLIS);
    }

    /// The same, `delay_millis` on, as `moveBy(delta, delayMillis)`.
    pub fn move_by_after(&mut self, delta: Offset, delay_millis: u64) {
        let position = self.current_position() + delta;
        self.move_to_after(position, delay_millis);
    }

    /// Moves the cursor without sending an event, as `updatePointerTo`.
    pub fn update_pointer_to(&mut self, position: Offset) {
        let root = self.0.to_root(position);
        self.input().mouse = root;
    }

    /// As `updatePointerBy`.
    pub fn update_pointer_by(&mut self, delta: Offset) {
        let position = self.current_position() + delta;
        self.update_pointer_to(position);
    }

    /// Presses `button` where the cursor is, as `press(button)`.
    #[track_caller]
    pub fn press(&mut self, button: MouseButton) {
        self.0.cursor_press(MOUSE, button.bit(), &button.button_id().to_string());
    }

    /// Releases `button`, as `release(button)`: the cursor hovers again.
    #[track_caller]
    pub fn release(&mut self, button: MouseButton) {
        self.0.cursor_release(MOUSE, button.bit(), &button.button_id().to_string());
    }

    /// Cancels the press, an event period on, as `cancel()`.
    #[track_caller]
    pub fn cancel(&mut self) {
        self.0.cursor_cancel(MOUSE, EVENT_PERIOD_MILLIS);
    }

    /// The cursor comes into the window at `position`, an event period on, as `enter`.
    #[track_caller]
    pub fn enter(&mut self, position: Offset) {
        self.0.cursor_enter(MOUSE, position, EVENT_PERIOD_MILLIS);
    }

    /// The cursor leaves the window at `position`, an event period on, as `exit`.
    #[track_caller]
    pub fn exit(&mut self, position: Offset) {
        self.0.cursor_exit(MOUSE, position, EVENT_PERIOD_MILLIS);
    }

    /// Turns `wheel` by `delta` (positive: forward, new content at the bottom or end; a unit
    /// is 64 dp, as on Android), after a move to where the cursor is, as `scroll`.
    pub fn scroll(&mut self, delta: f32, wheel: ScrollWheel) {
        let position = self.current_position();
        self.move_to_after(position, 0);
        let root = self.input().mouse;
        if !self.0.within_root(root) {
            return;
        }
        let pixels = self.0.density.to_px(Dp(delta * SCROLL_DP_PER_UNIT));
        let delta = match wheel {
            ScrollWheel::Vertical => Offset::new(0.0, pixels),
            ScrollWheel::Horizontal => Offset::new(pixels, 0.0),
        };
        self.mouse_event(PointerEvent::scroll(root, delta));
    }

    /// Turns both wheels by `offset` in one event (units as `scroll`'s), as `scroll(offset:
    /// Offset)`.
    pub fn scroll_offset(&mut self, offset: Offset) {
        let position = self.current_position();
        self.move_to_after(position, 0);
        let root = self.input().mouse;
        if !self.0.within_root(root) {
            return;
        }
        let density = self.0.density;
        let to_px = |units: f32| density.to_px(Dp(units * SCROLL_DP_PER_UNIT));
        let delta = Offset::new(to_px(offset.x), to_px(offset.y));
        self.mouse_event(PointerEvent::scroll(root, delta));
    }

    // ---- Gestures -------------------------------------------------------------------------

    /// A click at the center, the button held 60 ms, as `click()`.
    pub fn click(&mut self) {
        let center = self.center();
        self.click_at(center, MouseButton::Primary);
    }

    /// A click at `position` with `button`, as `click(position, button)`.
    #[track_caller]
    pub fn click_at(&mut self, position: Offset, button: MouseButton) {
        self.update_pointer_to(position);
        self.press(button);
        self.0.advance_event_time(SINGLE_CLICK_DELAY_MILLIS);
        self.release(button);
    }

    /// A secondary-button click, as `rightClick`: compose-rs has no secondary button, so it
    /// panics (D903).
    #[track_caller]
    pub fn right_click(&mut self, position: Offset) {
        self.click_at(position, MouseButton::Secondary);
    }

    /// Two clicks, halfway through the double-tap window apart, as `doubleClick`.
    pub fn double_click(&mut self, position: Offset) {
        let delay = default_double_tap_delay_millis(&self.view_configuration());
        self.click_at(position, MouseButton::Primary);
        self.0.advance_event_time(delay);
        self.click_at(position, MouseButton::Primary);
    }

    /// Three clicks, as `tripleClick`.
    pub fn triple_click(&mut self, position: Offset) {
        let delay = default_double_tap_delay_millis(&self.view_configuration());
        self.click_at(position, MouseButton::Primary);
        self.0.advance_event_time(delay);
        self.click_at(position, MouseButton::Primary);
        self.0.advance_event_time(delay);
        self.click_at(position, MouseButton::Primary);
    }

    /// A press held 100 ms past the long-press timeout, as `longClick`.
    pub fn long_click(&mut self, position: Offset) {
        let hold = self.view_configuration().long_press_timeout_millis + 100;
        self.update_pointer_to(position);
        self.press(MouseButton::Primary);
        self.0.advance_event_time(hold);
        self.release(MouseButton::Primary);
    }

    /// Moves the cursor to `position` over `duration_millis`, a move about every event period,
    /// as `animateMoveTo`.
    pub fn animate_move_to(&mut self, position: Offset, duration_millis: u64) {
        let start = self.current_position();
        let duration = duration_millis as f32;
        self.animate_move_along(
            &|time| lerp(start, position, time as f32 / duration),
            duration_millis,
        );
    }

    /// As `animateMoveBy`.
    pub fn animate_move_by(&mut self, delta: Offset, duration_millis: u64) {
        let position = self.current_position() + delta;
        self.animate_move_to(position, duration_millis);
    }

    /// Moves the cursor along `curve` over `duration_millis`, as `animateMoveAlong`.
    #[track_caller]
    pub fn animate_move_along(&mut self, curve: &dyn Fn(u64) -> Offset, duration_millis: u64) {
        self.0.cursor_animate_along(MOUSE, curve, duration_millis);
    }

    /// Presses at `start`, moves to `end` over `duration_millis` and releases, as
    /// `dragAndDrop`.
    pub fn drag_and_drop(&mut self, start: Offset, end: Offset, duration_millis: u64) {
        self.update_pointer_to(start);
        self.press(MouseButton::Primary);
        self.animate_move_to(end, duration_millis);
        self.release(MouseButton::Primary);
    }

    /// Scrolls `amount` in steps about an event period apart over `duration_millis`, as
    /// `smoothScroll`.
    pub fn smooth_scroll(&mut self, amount: f32, duration_millis: u64, wheel: ScrollWheel) {
        let steps = ((duration_millis as f32 / EVENT_PERIOD_MILLIS as f32).round() as u64).max(1);
        let (mut previous_time, mut previous_value) = (0, 0.0);
        for step in 1..=steps {
            let progress = step as f32 / steps as f32;
            let time = lerp_time(0, duration_millis, progress);
            let value = amount * progress;
            self.0.advance_event_time(time - previous_time);
            self.scroll(value - previous_value, wheel);
            previous_time = time;
            previous_value = value;
        }
    }

    /// As [`InjectionScope::advance_event_time`].
    pub fn advance_event_time(&mut self, duration_millis: u64) {
        self.0.advance_event_time(duration_millis);
    }

    /// The default duration of the mouse's animated gestures (300 ms).
    pub fn default_gesture_millis(&self) -> u64 {
        DEFAULT_MOUSE_GESTURE_MILLIS
    }
}

// ---- Trackpad ----------------------------------------------------------------------------------

/// A trackpad's button, as Compose's `TrackpadButton`: its id is Android's `MotionEvent` bit.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum TrackpadButton {
    #[default]
    Primary,
    Secondary,
    Tertiary,
}

impl TrackpadButton {
    /// Its id, as `buttonId`: its bit among an event's buttons.
    pub fn button_id(self) -> i32 {
        self.bit().0
    }

    fn bit(self) -> PointerButtons {
        match self {
            TrackpadButton::Primary => PointerButtons::PRIMARY,
            TrackpadButton::Secondary => PointerButtons::SECONDARY,
            TrackpadButton::Tertiary => PointerButtons::TERTIARY,
        }
    }
}

/// A trackpad's gestures on a node, as Compose's `TrackpadInjectionScope`: the cursor it moves
/// (the mouse's), its buttons, and its two-finger pans and pinches, positions in the node's
/// coordinates.
///
/// What a node hears is what Compose's hears on Android 14 and later (D1674): a pan is
/// `PanStart`, `PanMove`s (their `pan_offset` the scroll's, the opposite of the fingers' move, as
/// Android's `AXIS_GESTURE_SCROLL_*_DISTANCE`) and `PanEnd`, at the cursor, which keeps hovering;
/// a pinch is `ScaleStart`, `ScaleChange`s (one with its second finger down and one with it up,
/// a factor of 1) and `ScaleEnd`; a gesture starting with buttons held releases them first, and
/// after one the cursor hovers again with a move where it is.
pub struct TrackpadInjectionScope<'r>(InjectionScope<'r>);

impl<'r> Deref for TrackpadInjectionScope<'r> {
    type Target = InjectionScope<'r>;
    fn deref(&self) -> &InjectionScope<'r> {
        &self.0
    }
}

const TRACKPAD: CursorSource = CursorSource::Trackpad;

/// A trackpad gesture's default duration, as Compose's `DefaultTrackpadGestureDurationMillis`.
const DEFAULT_TRACKPAD_GESTURE_MILLIS: u64 = 300;

impl TrackpadInjectionScope<'_> {
    fn input(&mut self) -> &mut InputState {
        &mut self.0.rule.input
    }

    /// The cursor's position, in the node's coordinates, as `currentPosition`.
    pub fn current_position(&self) -> Offset {
        self.0.cursor_position()
    }

    /// Moves the cursor to `position`, an event period on, as `moveTo(position)`.
    pub fn move_to(&mut self, position: Offset) {
        self.move_to_after(position, EVENT_PERIOD_MILLIS);
    }

    /// The same, `delay_millis` on, as `moveTo(position, delayMillis)`.
    pub fn move_to_after(&mut self, position: Offset, delay_millis: u64) {
        self.0.cursor_move_to(TRACKPAD, position, delay_millis);
    }

    /// As `moveBy(delta)`.
    pub fn move_by(&mut self, delta: Offset) {
        self.move_by_after(delta, EVENT_PERIOD_MILLIS);
    }

    /// As `moveBy(delta, delayMillis)`.
    pub fn move_by_after(&mut self, delta: Offset, delay_millis: u64) {
        let position = self.current_position() + delta;
        self.move_to_after(position, delay_millis);
    }

    /// Moves the cursor without sending an event, as `updatePointerTo`.
    pub fn update_pointer_to(&mut self, position: Offset) {
        let root = self.0.to_root(position);
        self.input().mouse = root;
    }

    /// As `updatePointerBy`.
    pub fn update_pointer_by(&mut self, delta: Offset) {
        let position = self.current_position() + delta;
        self.update_pointer_to(position);
    }

    /// Presses `button` where the cursor is, as `press(button)`.
    #[track_caller]
    pub fn press(&mut self, button: TrackpadButton) {
        self.0.cursor_press(TRACKPAD, button.bit(), &button.button_id().to_string());
    }

    /// Releases `button`, as `release(button)`: the cursor hovers again.
    #[track_caller]
    pub fn release(&mut self, button: TrackpadButton) {
        self.0.cursor_release(TRACKPAD, button.bit(), &button.button_id().to_string());
    }

    /// Cancels the buttons' gesture, an event period on, as `cancel()`.
    #[track_caller]
    pub fn cancel(&mut self) {
        self.cancel_after(EVENT_PERIOD_MILLIS);
    }

    /// As `cancel(delayMillis)`.
    #[track_caller]
    pub fn cancel_after(&mut self, delay_millis: u64) {
        self.0.cursor_cancel(TRACKPAD, delay_millis);
    }

    /// The cursor comes into the window at `position`, an event period on, as `enter`.
    #[track_caller]
    pub fn enter(&mut self, position: Offset) {
        self.enter_after(position, EVENT_PERIOD_MILLIS);
    }

    /// As `enter(position, delayMillis)`.
    #[track_caller]
    pub fn enter_after(&mut self, position: Offset, delay_millis: u64) {
        self.0.cursor_enter(TRACKPAD, position, delay_millis);
    }

    /// The cursor leaves the window at `position`, an event period on, as `exit`.
    #[track_caller]
    pub fn exit(&mut self, position: Offset) {
        self.exit_after(position, EVENT_PERIOD_MILLIS);
    }

    /// As `exit(position, delayMillis)`.
    #[track_caller]
    pub fn exit_after(&mut self, position: Offset, delay_millis: u64) {
        self.0.cursor_exit(TRACKPAD, position, delay_millis);
    }

    /// What a pan or a pinch starts with, inside the window: the buttons held are released (and
    /// the cursor hovers again), as Compose's on Android, whose two fingers are a touch stream.
    fn release_buttons(&mut self) {
        if !self.input().mouse_buttons.are_any_pressed() {
            return;
        }
        self.input().mouse_buttons = PointerButtons::NONE;
        let position = self.input().mouse;
        self.0.cursor_event(PointerEvent::release(position).with_buttons(PointerButtons::NONE));
        self.0.cursor_event(PointerEvent::moved(position, false));
    }

    /// The cursor hovers again after a pan or a pinch, as Compose's hover enter after them.
    fn hover_again(&mut self) {
        let position = self.input().mouse;
        self.0.cursor_event(PointerEvent::moved(position, false));
    }

    fn cursor_inside(&self) -> bool {
        self.0.within_root(self.0.rule.input.mouse)
    }

    /// Starts a two-finger pan where the cursor is, as `panStart`.
    #[track_caller]
    pub fn pan_start(&mut self) {
        self.input().cursor_source = Some(TRACKPAD);
        assert!(
            self.input().pan.is_none(),
            "compose-test: Cannot send trackpad pan start event, a pan gesture is already in \
             progress"
        );
        self.input().pan = Some(Offset::ZERO);
        if self.cursor_inside() {
            self.release_buttons();
            let position = self.input().mouse;
            self.0.cursor_event(PointerEvent::pan(
                PointerEventType::PanStart,
                position,
                Offset::ZERO,
            ));
        }
    }

    /// Pans by `delta` (the fingers' move), an event period on, as `panMoveBy(delta)`.
    #[track_caller]
    pub fn pan_move_by(&mut self, delta: Offset) {
        self.pan_move_by_after(delta, EVENT_PERIOD_MILLIS);
    }

    /// As `panMoveBy(delta, delayMillis)`.
    #[track_caller]
    pub fn pan_move_by_after(&mut self, delta: Offset, delay_millis: u64) {
        self.0.advance_event_time(delay_millis);
        self.input().cursor_source = Some(TRACKPAD);
        let Some(panned) = self.input().pan else {
            panic!(
                "compose-test: Cannot send trackpad pan move event, no pan gesture is in progress"
            )
        };
        self.input().pan = Some(panned + delta);
        if self.cursor_inside() {
            let position = self.input().mouse;
            // The scroll's distance, the fingers' move turned around, as Android's.
            let offset = Offset::ZERO - delta;
            self.0.cursor_event(PointerEvent::pan(PointerEventType::PanMove, position, offset));
        }
    }

    /// Ends the pan, an event period on, as `panEnd()`.
    #[track_caller]
    pub fn pan_end(&mut self) {
        self.pan_end_after(EVENT_PERIOD_MILLIS);
    }

    /// As `panEnd(delayMillis)`.
    #[track_caller]
    pub fn pan_end_after(&mut self, delay_millis: u64) {
        self.0.advance_event_time(delay_millis);
        self.input().cursor_source = Some(TRACKPAD);
        assert!(
            self.input().pan.is_some(),
            "compose-test: Cannot send trackpad pan end event, no pan gesture is in progress"
        );
        if self.cursor_inside() {
            let position = self.input().mouse;
            self.0.cursor_event(PointerEvent::pan(
                PointerEventType::PanEnd,
                position,
                Offset::ZERO,
            ));
            self.hover_again();
        }
        self.input().pan = None;
    }

    /// Starts a pinch where the cursor is, as `scaleStart`.
    #[track_caller]
    pub fn scale_start(&mut self) {
        self.input().cursor_source = Some(TRACKPAD);
        assert!(
            self.input().scale.is_none(),
            "compose-test: Cannot send trackpad scale start event, a scale gesture is already in \
             progress"
        );
        self.input().scale = Some(1.0);
        if self.cursor_inside() {
            self.release_buttons();
            let position = self.input().mouse;
            self.0.cursor_event(PointerEvent::scale(PointerEventType::ScaleStart, position, 1.0));
            // The second finger coming down.
            self.0.cursor_event(PointerEvent::scale(PointerEventType::ScaleChange, position, 1.0));
        }
    }

    /// Scales by `scale_factor` (more than 0), an event period on, as `scaleChangeBy`.
    #[track_caller]
    pub fn scale_change_by(&mut self, scale_factor: f32) {
        self.scale_change_by_after(scale_factor, EVENT_PERIOD_MILLIS);
    }

    /// As `scaleChangeBy(scaleFactor, delayMillis)`.
    #[track_caller]
    pub fn scale_change_by_after(&mut self, scale_factor: f32, delay_millis: u64) {
        self.0.advance_event_time(delay_millis);
        self.input().cursor_source = Some(TRACKPAD);
        let Some(scaled) = self.input().scale else {
            panic!(
                "compose-test: Cannot send trackpad scale change event, no scale gesture is in \
                 progress"
            )
        };
        self.input().scale = Some(scaled * scale_factor);
        if self.cursor_inside() {
            let position = self.input().mouse;
            let event = PointerEvent::scale(PointerEventType::ScaleChange, position, scale_factor);
            self.0.cursor_event(event);
        }
    }

    /// Ends the pinch, an event period on, as `scaleEnd()`.
    #[track_caller]
    pub fn scale_end(&mut self) {
        self.scale_end_after(EVENT_PERIOD_MILLIS);
    }

    /// As `scaleEnd(delayMillis)`.
    #[track_caller]
    pub fn scale_end_after(&mut self, delay_millis: u64) {
        self.0.advance_event_time(delay_millis);
        self.input().cursor_source = Some(TRACKPAD);
        assert!(
            self.input().scale.is_some(),
            "compose-test: Cannot send trackpad scale end event, no scale gesture is in progress"
        );
        if self.cursor_inside() {
            let position = self.input().mouse;
            // The second finger lifting, then the first.
            self.0.cursor_event(PointerEvent::scale(PointerEventType::ScaleChange, position, 1.0));
            self.0.cursor_event(PointerEvent::scale(PointerEventType::ScaleEnd, position, 1.0));
            self.hover_again();
        }
        self.input().scale = None;
    }

    // ---- Gestures -------------------------------------------------------------------------

    /// A click at the center with the primary button, as `click()`.
    pub fn click(&mut self) {
        let center = self.center();
        self.click_at(center, TrackpadButton::Primary);
    }

    /// A press and a release of `button` at `position`, as `click(position, button)`.
    #[track_caller]
    pub fn click_at(&mut self, position: Offset, button: TrackpadButton) {
        self.update_pointer_to(position);
        self.press(button);
        self.release(button);
    }

    /// A secondary-button click, as `rightClick(position)`.
    #[track_caller]
    pub fn right_click(&mut self, position: Offset) {
        self.click_at(position, TrackpadButton::Secondary);
    }

    /// Two clicks, halfway through the double-tap window apart, as `doubleClick`.
    pub fn double_click(&mut self, position: Offset, button: TrackpadButton) {
        let delay = default_double_tap_delay_millis(&self.view_configuration());
        self.click_at(position, button);
        self.0.advance_event_time(delay);
        self.click_at(position, button);
    }

    /// Three clicks, as `tripleClick`.
    pub fn triple_click(&mut self, position: Offset, button: TrackpadButton) {
        let delay = default_double_tap_delay_millis(&self.view_configuration());
        self.click_at(position, button);
        self.0.advance_event_time(delay);
        self.click_at(position, button);
        self.0.advance_event_time(delay);
        self.click_at(position, button);
    }

    /// A press held 100 ms past the long-press timeout, as `longClick`.
    pub fn long_click(&mut self, position: Offset, button: TrackpadButton) {
        let hold = self.view_configuration().long_press_timeout_millis + 100;
        self.update_pointer_to(position);
        self.press(button);
        self.0.advance_event_time(hold);
        self.release(button);
    }

    /// Moves the cursor to `position` over `duration_millis`, as `animateMoveTo`.
    pub fn animate_move_to(&mut self, position: Offset, duration_millis: u64) {
        let start = self.current_position();
        let duration = duration_millis as f32;
        self.animate_move_along(
            &|time| lerp(start, position, time as f32 / duration),
            duration_millis,
        );
    }

    /// As `animateMoveBy`.
    pub fn animate_move_by(&mut self, delta: Offset, duration_millis: u64) {
        let position = self.current_position() + delta;
        self.animate_move_to(position, duration_millis);
    }

    /// Moves the cursor along `curve` over `duration_millis`, as `animateMoveAlong`.
    #[track_caller]
    pub fn animate_move_along(&mut self, curve: &dyn Fn(u64) -> Offset, duration_millis: u64) {
        self.0.cursor_animate_along(TRACKPAD, curve, duration_millis);
    }

    /// Presses `button` at `start`, moves to `end` over `duration_millis` and releases, as
    /// `dragAndDrop`.
    pub fn drag_and_drop(
        &mut self,
        start: Offset,
        end: Offset,
        button: TrackpadButton,
        duration_millis: u64,
    ) {
        self.update_pointer_to(start);
        self.press(button);
        self.animate_move_to(end, duration_millis);
        self.release(button);
    }

    /// A pan by `offset` in one move, as `pan(offset)`.
    pub fn pan(&mut self, offset: Offset) {
        self.pan_start();
        self.pan_move_by(offset);
        self.pan_end();
    }

    /// A pan along `curve` (how far it's panned at each time, in ms, from zero) over
    /// `duration_millis`, a move about every event period and at each key time, as
    /// `pan(curve, durationMillis, keyTimes)`.
    #[track_caller]
    pub fn pan_along(
        &mut self,
        curve: &dyn Fn(u64) -> Offset,
        duration_millis: u64,
        key_times: &[u64],
    ) {
        assert!(
            duration_millis >= 1,
            "compose-test: duration must be at least 1 millisecond, not {duration_millis}"
        );
        assert!(
            key_times.iter().all(|time| *time <= duration_millis),
            "compose-test: keyTimes contains timestamps out of range [0..{duration_millis}]: \
             {key_times:?}"
        );
        assert!(
            key_times.windows(2).all(|pair| pair[0] <= pair[1]),
            "compose-test: keyTimes must be sorted: {key_times:?}"
        );
        self.pan_start();
        let mut panned = Offset::ZERO;
        let (mut time, mut key) = (0, 0);
        while time < duration_millis {
            while key < key_times.len() && key_times[key] <= time {
                key += 1;
            }
            let next = key_times.get(key).copied().unwrap_or(duration_millis);
            let steps = (((next - time) as f32 / EVENT_PERIOD_MILLIS as f32).round() as u64).max(1);
            let mut previous = time;
            for step in 1..=steps {
                let at = lerp_time(time, next, step as f32 / steps as f32);
                let value = curve(at);
                self.pan_move_by_after(value - panned, at - previous);
                panned = value;
                previous = at;
            }
            time = next;
        }
        self.pan_end_after(0);
    }

    /// A pan by `offset` ending at `end_velocity` px/s, as `panWithVelocity`, over a default
    /// duration computed from it (`None`), along the path of `swipeWithVelocity`.
    #[track_caller]
    pub fn pan_with_velocity(
        &mut self,
        offset: Offset,
        end_velocity: f32,
        duration_millis: Option<u64>,
    ) {
        assert!(
            end_velocity >= 0.0,
            "compose-test: Velocity cannot be {end_velocity}, it must be positive"
        );
        let duration = duration_millis
            .unwrap_or_else(|| default_velocity_duration(Offset::ZERO, offset, end_velocity));
        let minimum = (2.5 * EVENT_PERIOD_MILLIS as f32).ceil() as u64;
        assert!(
            duration >= minimum,
            "compose-test: Duration must be at least {minimum}ms because velocity requires at \
             least 3 input events"
        );
        let path = VelocityPath::new(Offset::ZERO, offset, end_velocity, duration);
        self.pan_along(&|time| path.offset_at(time), duration, &[]);
    }

    /// A pinch scaling by `scale_factor` in one change, as `scale(scaleFactor)`.
    pub fn scale(&mut self, scale_factor: f32) {
        self.scale_start();
        self.scale_change_by(scale_factor);
        self.scale_end();
    }

    /// As [`InjectionScope::advance_event_time`].
    pub fn advance_event_time(&mut self, duration_millis: u64) {
        self.0.advance_event_time(duration_millis);
    }

    /// The default duration of the trackpad's animated gestures (300 ms).
    pub fn default_gesture_millis(&self) -> u64 {
        DEFAULT_TRACKPAD_GESTURE_MILLIS
    }
}

// ---- Keys --------------------------------------------------------------------------------------

/// Key presses, as Compose's `KeyInjectionScope`: to the focused node, the modifier keys held
/// set the events' modifiers, a held key repeats (after 500 ms, then every 50 ms).
pub struct KeyInjectionScope<'r>(InjectionScope<'r>);

impl<'r> Deref for KeyInjectionScope<'r> {
    type Target = InjectionScope<'r>;
    fn deref(&self) -> &InjectionScope<'r> {
        &self.0
    }
}

impl KeyInjectionScope<'_> {
    fn input(&mut self) -> &mut InputState {
        &mut self.0.rule.input
    }

    /// Presses `key` down, as `keyDown`.
    #[track_caller]
    pub fn key_down(&mut self, key: Key) {
        assert!(
            !self.is_key_down(key),
            "compose-test: Cannot send key down event, Key({key:?}) is already pressed down."
        );
        let now = self.0.rule.current_time_millis();
        let input = self.input();
        input.key_down_time = now;
        input.keys.push(key);
        input.repeat_key = Some(key);
        input.last_repeat_time = now;
        toggle_lock(input, key);
        self.0.send_key(key, KeyEventType::KeyDown);
    }

    /// Releases `key`, as `keyUp`.
    #[track_caller]
    pub fn key_up(&mut self, key: Key) {
        assert!(
            self.is_key_down(key),
            "compose-test: Cannot send key up event, Key({key:?}) is not pressed down."
        );
        let input = self.input();
        input.keys.retain(|held| *held != key);
        if input.repeat_key == Some(key) {
            input.repeat_key = None;
        }
        toggle_lock(input, key);
        self.0.send_key(key, KeyEventType::KeyUp);
    }

    /// Whether `key` is held, as `isKeyDown`.
    pub fn is_key_down(&self, key: Key) -> bool {
        self.0.rule.input.keys.contains(&key)
    }

    /// Presses and releases `key`, held 50 ms, as `pressKey(key)`.
    pub fn press_key(&mut self, key: Key) {
        self.press_key_for(key, DEFAULT_KEY_PRESS_MILLIS);
    }

    /// Presses `key` and holds it `duration_millis` (repeating past 500 ms), as
    /// `pressKey(key, pressDurationMillis)`.
    pub fn press_key_for(&mut self, key: Key, duration_millis: u64) {
        self.key_down(key);
        self.0.advance_event_time(duration_millis);
        self.key_up(key);
    }

    /// Runs `block` with `key` held, as `withKeyDown`.
    pub fn with_key_down(&mut self, key: Key, block: impl FnOnce(&mut Self)) {
        self.with_keys_down(&[key], block);
    }

    /// Runs `block` with `keys` held, as `withKeysDown`: they go up after it, a panic in setup or
    /// in `block` too, releasing only what was pressed and still down (Compose's `try … finally`,
    /// D2079, D2132).
    pub fn with_keys_down(&mut self, keys: &[Key], block: impl FnOnce(&mut Self)) {
        let mut pressed = 0;
        let setup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for &key in keys {
                let was_down = self.is_key_down(key);
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.key_down(key);
                }));
                if !was_down && self.is_key_down(key) {
                    pressed += 1;
                }
                if let Err(panic) = res {
                    std::panic::resume_unwind(panic);
                }
            }
        }));
        if let Err(panic) = setup {
            self.release_keys_quietly(&keys[..pressed]);
            std::panic::resume_unwind(panic);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| block(self)));
        if let Err(panic) = result {
            self.release_keys_quietly(keys);
            std::panic::resume_unwind(panic);
        }
        for key in keys {
            self.key_up(*key);
        }
    }

    fn release_keys_quietly(&mut self, keys: &[Key]) {
        for &key in keys {
            if self.is_key_down(key) {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.key_up(key);
                }));
            }
        }
    }

    /// Runs `block` with `key` toggled on (pressed before and after), as `withKeyToggled`.
    pub fn with_key_toggled(&mut self, key: Key, block: impl FnOnce(&mut Self)) {
        self.with_keys_toggled(&[key], block);
    }

    /// As `withKeysToggled`: toggled back after `block`, a panic in setup or in `block` too
    /// (D2079, D2132).
    pub fn with_keys_toggled(&mut self, keys: &[Key], block: impl FnOnce(&mut Self)) {
        let mut toggled = 0;
        let setup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for (index, &key) in keys.iter().enumerate() {
                if index > 0 {
                    self.0.advance_event_time(DEFAULT_KEY_PRESS_MILLIS);
                }
                let was_down = self.is_key_down(key);
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.press_key(key);
                }));
                if let Err(panic) = res {
                    if !was_down && self.is_key_down(key) {
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            self.key_up(key);
                        }));
                        toggled += 1;
                    }
                    std::panic::resume_unwind(panic);
                }
                toggled += 1;
            }
        }));
        if let Err(panic) = setup {
            self.toggle_keys_quietly(&keys[..toggled]);
            std::panic::resume_unwind(panic);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| block(self)));
        if let Err(panic) = result {
            self.toggle_keys_quietly(keys);
            std::panic::resume_unwind(panic);
        }
        self.press_keys(keys);
    }

    fn toggle_keys_quietly(&mut self, keys: &[Key]) {
        for (index, &key) in keys.iter().enumerate() {
            if index > 0 {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.0.advance_event_time(DEFAULT_KEY_PRESS_MILLIS);
                }));
            }
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if self.is_key_down(key) {
                    self.key_up(key);
                } else {
                    self.press_key(key);
                }
            }));
        }
    }

    fn press_keys(&mut self, keys: &[Key]) {
        for (index, key) in keys.iter().enumerate() {
            if index > 0 {
                self.0.advance_event_time(DEFAULT_KEY_PRESS_MILLIS);
            }
            self.press_key(*key);
        }
    }

    pub fn is_shift_down(&self) -> bool {
        self.is_key_down(Key::ShiftLeft) || self.is_key_down(Key::ShiftRight)
    }

    pub fn is_ctrl_down(&self) -> bool {
        self.is_key_down(Key::CtrlLeft) || self.is_key_down(Key::CtrlRight)
    }

    pub fn is_alt_down(&self) -> bool {
        self.is_key_down(Key::AltLeft) || self.is_key_down(Key::AltRight)
    }

    pub fn is_meta_down(&self) -> bool {
        self.is_key_down(Key::MetaLeft) || self.is_key_down(Key::MetaRight)
    }

    /// Whether Caps Lock is on, from its first press's down to its second press's up, as
    /// `isCapsLockOn` (Android's behaviour).
    pub fn is_caps_lock_on(&self) -> bool {
        self.0.rule.input.caps_lock > 0
    }

    pub fn is_num_lock_on(&self) -> bool {
        self.0.rule.input.num_lock > 0
    }

    pub fn is_scroll_lock_on(&self) -> bool {
        self.0.rule.input.scroll_lock > 0
    }

    /// As [`InjectionScope::advance_event_time`]: a held key repeats meanwhile.
    pub fn advance_event_time(&mut self, duration_millis: u64) {
        self.0.advance_event_time(duration_millis);
    }
}

/// A lock key turns its lock on at its first down and off at its second up, as Compose's
/// `LockKeyState` cycle (down: on; up; down; up: off).
fn toggle_lock(input: &mut InputState, key: Key) {
    let state = match key {
        Key::CapsLock => &mut input.caps_lock,
        Key::NumLock => &mut input.num_lock,
        Key::ScrollLock => &mut input.scroll_lock,
        _ => return,
    };
    *state = (*state + 1) % 4;
}

// ---- The interactions' entry points ---------------------------------------------------------------

impl SemanticsNodeInteraction<'_> {
    /// The node's visible bounds in the root (clipped), and the root's.
    #[track_caller]
    fn injection_bounds(&mut self) -> (NodeId, Rect, Rect) {
        let node = self.fetch_semantics_snapshot();
        let root = self.rule.host.semantics_tree(true).bounds;
        let bounds = self
            .rule
            .host
            .with_tree(|tree| tree.semantics_bounds_in_root(node.id))
            .map_or(node.bounds, |(_, clipped)| clipped);
        (node.id, bounds, root)
    }

    /// Sends the touch gestures `block` makes on the node, then waits for idle, as
    /// `performTouchInput`.
    #[track_caller]
    pub fn perform_touch_input(
        &mut self,
        block: impl FnOnce(&mut TouchInjectionScope<'_>),
    ) -> &mut Self {
        let (node, bounds, root) = self.injection_bounds();
        let mut scope = TouchInjectionScope(InjectionScope::new(self.rule, node, bounds, root));
        block(&mut scope);
        self.rule.wait_for_idle();
        self
    }

    /// Sends the mouse gestures `block` makes on the node, then waits for idle, as
    /// `performMouseInput`.
    #[track_caller]
    pub fn perform_mouse_input(
        &mut self,
        block: impl FnOnce(&mut MouseInjectionScope<'_>),
    ) -> &mut Self {
        let (node, bounds, root) = self.injection_bounds();
        let mut scope = MouseInjectionScope(InjectionScope::new(self.rule, node, bounds, root));
        block(&mut scope);
        self.rule.wait_for_idle();
        self
    }

    /// Sends the trackpad gestures `block` makes on the node, then waits for idle, as
    /// `performTrackpadInput`.
    #[track_caller]
    pub fn perform_trackpad_input(
        &mut self,
        block: impl FnOnce(&mut TrackpadInjectionScope<'_>),
    ) -> &mut Self {
        let (node, bounds, root) = self.injection_bounds();
        let mut scope = TrackpadInjectionScope(InjectionScope::new(self.rule, node, bounds, root));
        block(&mut scope);
        self.rule.wait_for_idle();
        self
    }

    /// Sends the key presses `block` makes (to the focused node), then waits for idle, as
    /// `performKeyInput`.
    #[track_caller]
    pub fn perform_key_input(
        &mut self,
        block: impl FnOnce(&mut KeyInjectionScope<'_>),
    ) -> &mut Self {
        let (node, bounds, root) = self.injection_bounds();
        let mut scope = KeyInjectionScope(InjectionScope::new(self.rule, node, bounds, root));
        block(&mut scope);
        self.rule.wait_for_idle();
        self
    }
}
