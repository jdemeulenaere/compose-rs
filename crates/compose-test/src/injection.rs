//! Input injection, as Compose's `performTouchInput`, `performMouseInput` and
//! `performKeyInput` with their `InjectionScope`s: gestures in the node's coordinates, sent
//! through the host as real input events, each at its time on the test clock (the clock moves
//! as the events' times do, as Compose's `InputDispatcher` moves its main clock).

use std::collections::BTreeMap;
use std::ops::Deref;
use std::time::Duration;

use compose_ui::{
    Density, Dp, IntSize, Key, KeyEvent, KeyEventType, KeyModifiers, Offset, PointerEvent,
    PointerId, PointerType, Rect, ViewConfiguration,
};

use crate::interaction::SemanticsNodeInteraction;
use crate::rule::ComposeTestRule;

/// The time between two injected events, as Compose's `InputDispatcher.eventPeriodMillis`.
const EVENT_PERIOD_MILLIS: u64 = 16;
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
const DEFAULT_SWIPE_MILLIS: u64 = 200;
/// A wheel's step in pixels per unit of `scroll`'s delta, as Android's (64 dp).
const SCROLL_DP_PER_UNIT: f32 = 64.0;
/// How far back a velocity is measured, as Compose's `HorizonMilliseconds`.
const HORIZON_MILLIS: u64 = 100;

/// The double tap's default delay, halfway between its least and most time, as Compose's.
fn default_double_tap_delay_millis(configuration: &ViewConfiguration) -> u64 {
    (configuration.double_tap_min_time_millis + configuration.double_tap_timeout_millis) / 2
}

/// A mouse button, as Compose's `MouseButton`. compose-rs's pointer events carry the primary
/// button only (D903).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum MouseButton {
    #[default]
    Primary,
    Secondary,
    Tertiary,
}

/// A mouse's scroll wheel, as Compose's `ScrollWheel`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum ScrollWheel {
    #[default]
    Vertical,
    Horizontal,
}

/// What injection keeps across blocks: the touches down, the mouse, the keys held, as
/// Compose's `InputDispatcherState`. Positions are in the root.
#[derive(Default)]
pub(crate) struct InputState {
    /// The fingers down, by pointer id, and whether one moved since the last event.
    touches: BTreeMap<u64, Offset>,
    touch_updated: bool,
    mouse: Offset,
    mouse_entered: bool,
    mouse_pressed: bool,
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
}

impl<'r> InjectionScope<'r> {
    fn new(rule: &'r mut ComposeTestRule, bounds: Rect, root: Rect) -> InjectionScope<'r> {
        let density = rule.density();
        InjectionScope {
            rule,
            origin: Offset::new(bounds.left, bounds.top),
            visible_size: IntSize::new(
                compose_ui::platform::round_to_int(bounds.width()),
                compose_ui::platform::round_to_int(bounds.height()),
            ),
            root,
            density,
        }
    }

    /// The time between two events by default, as `eventPeriodMillis` (16 ms).
    pub fn event_period_millis(&self) -> u64 {
        EVENT_PERIOD_MILLIS
    }

    /// The density, as the scope's `Density`.
    pub fn density(&self) -> Density {
        self.density
    }

    /// As `viewConfiguration`.
    /// The host's, which its gesture detectors use (`UiHost::set_view_configuration`).
    pub fn view_configuration(&self) -> ViewConfiguration {
        self.rule.host.view_configuration()
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
        let event = event.at_time(self.now_nanos());
        self.rule.host.send_pointer(event);
        self.rule.run_current();
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
        self.rule.host.send_key(KeyEvent { key, kind, modifiers });
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
        if self.input().mouse_pressed {
            self.0.rule.input.mouse_pressed = false;
            self.0.rule.host.with_tree(|tree| tree.cancel_pointer(PointerId(0)));
        } else if self.input().mouse_entered {
            self.0.rule.input.mouse_entered = false;
            self.0.rule.host.send_pointer_left();
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

    /// One move event per finger down (compose-rs's pointer events carry one change each).
    fn send_move(&mut self) {
        self.input().touch_updated = false;
        let touches: Vec<(u64, Offset)> =
            self.input().touches.iter().map(|(id, position)| (*id, *position)).collect();
        for (id, position) in touches {
            self.touch_event(PointerEvent::moved(position, true), id);
        }
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
        let Some(position) = self.input().touches.get(&id).copied() else {
            panic!(
                "compose-test: Cannot send UP event for pointer {id}, it is not active in the \
                 current gesture"
            )
        };
        if self.input().touch_updated {
            self.send_move();
        }
        self.touch_event(PointerEvent::release(position), id);
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

fn lerp(start: Offset, end: Offset, fraction: f32) -> Offset {
    Offset::new(start.x + (end.x - start.x) * fraction, start.y + (end.y - start.y) * fraction)
}

/// Kotlin's `lerp(Long, Long, Float)`: rounded.
fn lerp_time(start: u64, end: u64, fraction: f32) -> u64 {
    start + ((end - start) as f64 * fraction as f64).round() as u64
}

/// The default duration of a swipe ending at `velocity`, as Compose's
/// `VelocityPathFinder.calculateDefaultDuration`.
#[track_caller]
fn default_velocity_duration(start: Offset, end: Offset, velocity: f32) -> u64 {
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
struct VelocityPath {
    start: Offset,
    end: Offset,
    duration: u64,
    vx: f64,
    vy: f64,
}

impl VelocityPath {
    fn new(start: Offset, end: Offset, velocity: f32, duration: u64) -> VelocityPath {
        let theta = ((end.y - start.y) as f64).atan2((end.x - start.x) as f64);
        VelocityPath {
            start,
            end,
            duration,
            vx: theta.cos() * velocity as f64 / 1000.0,
            vy: theta.sin() * velocity as f64 / 1000.0,
        }
    }

    fn offset_at(&self, time: u64) -> Offset {
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

impl MouseInjectionScope<'_> {
    fn input(&mut self) -> &mut InputState {
        &mut self.0.rule.input
    }

    fn mouse_event(&mut self, event: PointerEvent) {
        self.0.send(event.with_pointer(PointerId(0), PointerType::Mouse));
    }

    /// Ends a touch gesture in progress: a mouse event cancels it, as Compose's dispatcher.
    fn cancel_touch(&mut self) {
        if self.input().touches.is_empty() {
            return;
        }
        let ids: Vec<u64> = std::mem::take(&mut self.input().touches).into_keys().collect();
        for id in ids {
            self.0.rule.host.with_tree(|tree| tree.cancel_pointer(touch_pointer(id)));
        }
    }

    /// The cursor's position, in the node's coordinates, as `currentPosition`.
    pub fn current_position(&self) -> Offset {
        self.0.to_local(self.0.rule.input.mouse)
    }

    /// Moves the cursor to `position`, an event period on, as `moveTo(position)`: it enters
    /// the window first if it was outside, and leaves it if `position` is.
    pub fn move_to(&mut self, position: Offset) {
        self.move_to_after(position, EVENT_PERIOD_MILLIS);
    }

    /// The same, `delay_millis` on, as `moveTo(position, delayMillis)`.
    pub fn move_to_after(&mut self, position: Offset, delay_millis: u64) {
        self.0.advance_event_time(delay_millis);
        self.cancel_touch();
        let root = self.0.to_root(position);
        self.input().mouse = root;
        let inside = self.0.within_root(root);
        // Leaving the window: the exit, then the move, as Compose's `enqueueMouseMove`.
        if !inside && self.input().mouse_entered {
            self.input().mouse_entered = false;
            self.0.rule.host.send_pointer_left();
        }
        if inside && !self.input().mouse_pressed {
            self.input().mouse_entered = true;
        }
        let pressed = self.input().mouse_pressed;
        self.mouse_event(PointerEvent::moved(root, pressed));
    }

    /// Moves the cursor by `delta`, an event period on, as `moveBy(delta)`.
    pub fn move_by(&mut self, delta: Offset) {
        let position = self.current_position() + delta;
        self.move_to(position);
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

    #[track_caller]
    fn primary_only(button: MouseButton) {
        assert!(
            button == MouseButton::Primary,
            "compose-test: compose-rs's pointer events carry the primary button only, not \
             {button:?}"
        );
    }

    /// Presses `button` where the cursor is, as `press(button)`.
    #[track_caller]
    pub fn press(&mut self, button: MouseButton) {
        Self::primary_only(button);
        assert!(
            !self.input().mouse_pressed,
            "compose-test: Cannot send mouse button down event, button {button:?} is already \
             pressed"
        );
        let position = self.input().mouse;
        assert!(
            self.0.within_root(position),
            "compose-test: Cannot start a mouse gesture outside the Compose root bounds, mouse \
             position is {position:?}"
        );
        self.cancel_touch();
        self.input().mouse_pressed = true;
        // A press ends the hover until the release, as Compose's on Android (`exitHoverOnPress`).
        self.input().mouse_entered = false;
        self.mouse_event(PointerEvent::press(position));
    }

    /// Releases `button`, as `release(button)`: the cursor hovers again.
    #[track_caller]
    pub fn release(&mut self, button: MouseButton) {
        Self::primary_only(button);
        assert!(
            self.input().mouse_pressed,
            "compose-test: Cannot send mouse button up event, button {button:?} is not pressed"
        );
        self.input().mouse_pressed = false;
        let position = self.input().mouse;
        self.mouse_event(PointerEvent::release(position));
        // Inside the window, the cursor hovers again at once: the enter and a move where it
        // is, as Compose's `enqueueMouseRelease` on Android (`exitHoverOnPress`).
        let inside = self.0.within_root(position);
        self.input().mouse_entered = inside;
        if inside {
            self.mouse_event(PointerEvent::moved(position, false));
        }
    }

    /// Cancels the press, an event period on, as `cancel()`.
    #[track_caller]
    pub fn cancel(&mut self) {
        assert!(
            self.input().mouse_pressed,
            "compose-test: Cannot send mouse cancel event, no mouse buttons are pressed"
        );
        self.0.advance_event_time(EVENT_PERIOD_MILLIS);
        self.input().mouse_pressed = false;
        self.0.rule.host.with_tree(|tree| tree.cancel_pointer(PointerId(0)));
        self.0.rule.run_current();
    }

    /// The cursor comes into the window at `position`, an event period on, as `enter`.
    #[track_caller]
    pub fn enter(&mut self, position: Offset) {
        assert!(
            !self.input().mouse_entered,
            "compose-test: Cannot send mouse hover enter event, mouse is already hovering"
        );
        assert!(
            !self.input().mouse_pressed,
            "compose-test: Cannot send mouse hover enter event, mouse buttons are down"
        );
        self.0.advance_event_time(EVENT_PERIOD_MILLIS);
        let root = self.0.to_root(position);
        assert!(
            self.0.within_root(root),
            "compose-test: Cannot send mouse hover enter event, {position:?} is out of bounds"
        );
        self.input().mouse = root;
        self.input().mouse_entered = true;
        self.mouse_event(PointerEvent::moved(root, false));
    }

    /// The cursor leaves the window at `position`, an event period on, as `exit`.
    #[track_caller]
    pub fn exit(&mut self, position: Offset) {
        assert!(
            self.input().mouse_entered,
            "compose-test: Cannot send mouse hover exit event, mouse is not hovering"
        );
        self.0.advance_event_time(EVENT_PERIOD_MILLIS);
        let root = self.0.to_root(position);
        self.input().mouse = root;
        self.input().mouse_entered = false;
        self.0.rule.host.send_pointer_left();
        self.0.rule.run_current();
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
        assert!(duration_millis > 0, "compose-test: Duration is 0");
        let start = curve(0);
        if start != self.current_position() {
            self.move_to_after(start, 0);
        }
        let steps = ((duration_millis as f32 / EVENT_PERIOD_MILLIS as f32).round() as u64).max(1);
        let mut previous = 0;
        for step in 1..=steps {
            let time = lerp_time(0, duration_millis, step as f32 / steps as f32);
            self.move_to_after(curve(time), time - previous);
            previous = time;
        }
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

    /// Runs `block` with `keys` held, as `withKeysDown`.
    pub fn with_keys_down(&mut self, keys: &[Key], block: impl FnOnce(&mut Self)) {
        for key in keys {
            self.key_down(*key);
        }
        block(self);
        for key in keys {
            self.key_up(*key);
        }
    }

    /// Runs `block` with `key` toggled on (pressed before and after), as `withKeyToggled`.
    pub fn with_key_toggled(&mut self, key: Key, block: impl FnOnce(&mut Self)) {
        self.with_keys_toggled(&[key], block);
    }

    /// As `withKeysToggled`.
    pub fn with_keys_toggled(&mut self, keys: &[Key], block: impl FnOnce(&mut Self)) {
        self.press_keys(keys);
        block(self);
        self.press_keys(keys);
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
    fn injection_bounds(&mut self) -> (Rect, Rect) {
        let node = self.fetch_semantics_node();
        let root = self.rule.host.semantics_tree(true).bounds;
        let bounds = self
            .rule
            .host
            .with_tree(|tree| tree.semantics_bounds_in_root(node.id))
            .map_or(node.bounds, |(_, clipped)| clipped);
        (bounds, root)
    }

    /// Sends the touch gestures `block` makes on the node, then waits for idle, as
    /// `performTouchInput`.
    #[track_caller]
    pub fn perform_touch_input(
        &mut self,
        block: impl FnOnce(&mut TouchInjectionScope<'_>),
    ) -> &mut Self {
        let (bounds, root) = self.injection_bounds();
        let mut scope = TouchInjectionScope(InjectionScope::new(self.rule, bounds, root));
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
        let (bounds, root) = self.injection_bounds();
        let mut scope = MouseInjectionScope(InjectionScope::new(self.rule, bounds, root));
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
        let (bounds, root) = self.injection_bounds();
        let mut scope = KeyInjectionScope(InjectionScope::new(self.rule, bounds, root));
        block(&mut scope);
        self.rule.wait_for_idle();
        self
    }
}
