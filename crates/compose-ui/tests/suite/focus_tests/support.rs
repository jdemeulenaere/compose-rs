//! A scene for the focus tests: a content whose targets have names, and what happened to them.
//!
//! A test writes its content with `Scene`'s modifiers (`scene.target("a")` is a target called `a`
//! that can be requested and heard), and asks by name afterwards. A scene is a `Copy` handle the
//! content's callbacks and the test share. Leaked: a test's lifetime is short.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use compose_runtime::{Callback, MutableState, callback, mutable_state_of};

pub use compose_ui::platform::*;
pub use compose_ui::*;

pub use super::held::HeldTarget;
pub use crate::support::*;
pub use FocusDirection::{Down, Enter, Exit, Left, Next, Previous, Right, Up};
pub use FocusState::{Active, ActiveParent, Captured, Inactive};

type Listener = Callback<dyn Fn(FocusState)>;

#[derive(Default)]
struct Data {
    host: RefCell<Option<UiHost>>,
    manager: RefCell<Option<FocusManager>>,
    requesters: RefCell<HashMap<&'static str, FocusRequester>>,
    flags: RefCell<HashMap<&'static str, MutableState<bool>>>,
    listeners: RefCell<HashMap<&'static str, Listener>>,
    event_listeners: RefCell<HashMap<&'static str, Listener>>,
    last: RefCell<HashMap<&'static str, FocusState>>,
    heard: RefCell<HashMap<&'static str, Vec<FocusState>>>,
    events: RefCell<HashMap<&'static str, Vec<FocusState>>>,
    log: RefCell<Vec<String>>,
}

/// A content and what it heard.
#[derive(Clone, Copy)]
pub struct Scene(&'static Data);

/// What keeps a scene's composition: it ends with this, the handles staying valid.
pub struct Composed(Scene);

impl std::ops::Deref for Composed {
    type Target = Scene;
    fn deref(&self) -> &Scene {
        &self.0
    }
}

impl Drop for Composed {
    fn drop(&mut self) {
        drop(self.0.0.host.borrow_mut().take());
    }
}

/// A scene of `content`, composed and settled, in a window of 1000 by 1000 pixels.
pub fn scene(content: impl Fn(Scene) + 'static) -> Composed {
    Scene::empty().compose(content)
}

impl Scene {
    /// A scene with no content yet: a test that needs its names before composing.
    pub fn empty() -> Scene {
        Scene(Box::leak(Box::default()))
    }

    /// Composes `content`.
    pub fn compose(self, content: impl Fn(Scene) + 'static) -> Composed {
        let host = UiHost::headless(1000, 1000);
        host.set_content(move || {
            *self.0.manager.borrow_mut() = Some(LOCAL_FOCUS_MANAGER.current());
            content(self);
        });
        *self.0.host.borrow_mut() = Some(host);
        self.idle();
        Composed(self)
    }
}

// ---- What a content is made of ------------------------------------------------------------------

impl Scene {
    /// The requester called `name`.
    pub fn requester(self, name: &'static str) -> FocusRequester {
        *self.0.requesters.borrow_mut().entry(name).or_default()
    }

    /// A state a content reads and a test writes, false at first.
    pub fn flag(self, name: &'static str) -> MutableState<bool> {
        *self.0.flags.borrow_mut().entry(name).or_insert_with(|| shared(false))
    }

    /// Hears `state` under `name`, as a listener does.
    pub fn hear(self, name: &'static str, state: FocusState) {
        self.0.last.borrow_mut().insert(name, state);
        self.0.heard.borrow_mut().entry(name).or_default().push(state);
    }

    fn hear_event(self, name: &'static str, state: FocusState) {
        self.0.last.borrow_mut().insert(name, state);
        self.0.events.borrow_mut().entry(name).or_default().push(state);
    }

    /// `on_focus_changed` heard under `name`, the same callback in every composition: an element
    /// that stays equal, and so a node that isn't told again.
    pub fn listen(self, name: &'static str) -> Modifier {
        let listener = self
            .0
            .listeners
            .borrow_mut()
            .entry(name)
            .or_insert_with(|| {
                callback::<dyn Fn(FocusState), _>(move |state: FocusState| self.hear(name, state))
            })
            .clone();
        Modifier.on_focus_changed(listener)
    }

    /// `on_focus_changed` heard under `name`, a new callback in each composition: the element
    /// updates and the node hears its target's state again.
    pub fn listen_anew(self, name: &'static str) -> Modifier {
        Modifier.on_focus_changed(move |state: FocusState| self.hear(name, state))
    }

    /// `on_focus_event` heard under `name`, the same callback in every composition.
    pub fn events(self, name: &'static str) -> Modifier {
        let listener = self
            .0
            .event_listeners
            .borrow_mut()
            .entry(name)
            .or_insert_with(|| {
                callback::<dyn Fn(FocusState), _>(move |state: FocusState| {
                    self.hear_event(name, state)
                })
            })
            .clone();
        Modifier.on_focus_event(listener)
    }

    /// `on_focus_event` heard under `name`, a new callback in each composition.
    pub fn events_anew(self, name: &'static str) -> Modifier {
        Modifier.on_focus_event(move |state: FocusState| self.hear_event(name, state))
    }

    /// `focus_requester` with the requester called `name`.
    pub fn req(self, name: &'static str) -> Modifier {
        Modifier.focus_requester(self.requester(name))
    }

    /// A requester called `name` and a listener of that name, for the target after them.
    pub fn named(self, name: &'static str) -> Modifier {
        self.req(name).then(self.listen(name))
    }

    /// A target called `name`.
    pub fn target(self, name: &'static str) -> Modifier {
        self.named(name).focus_target()
    }

    /// A target called `name` that can't take the focus itself.
    pub fn deactivated(self, name: &'static str) -> Modifier {
        self.named(name).then(cant_focus()).focus_target()
    }

    /// A target called `name`, at a place of its parent's.
    pub fn target_at(self, name: &'static str, place: Bounds) -> Modifier {
        place.modifier().then(self.target(name))
    }

    /// Writes to the log, for what has to be read in order across several names.
    pub fn say(self, entry: impl Into<String>) {
        self.0.log.borrow_mut().push(entry.into());
    }
}

/// `focus_properties` saying a target can't take the focus.
pub fn cant_focus() -> Modifier {
    Modifier.focus_properties(|properties: &mut FocusProperties| properties.can_focus = false)
}

/// `focus_properties` with `block`.
pub fn properties(block: impl Fn(&mut FocusProperties) + 'static) -> Modifier {
    Modifier.focus_properties(block)
}

/// Bounds in pixels in a parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Bounds from their left, top, right and bottom.
pub const fn ltrb(left: i32, top: i32, right: i32, bottom: i32) -> Bounds {
    Bounds { left, top, right, bottom }
}

impl Bounds {
    pub fn modifier(self) -> Modifier {
        Modifier.offset((self.left as f32).dp(), (self.top as f32).dp()).required_size((
            ((self.right - self.left) as f32).dp(),
            ((self.bottom - self.top) as f32).dp(),
        ))
    }
}

/// An interval on an axis of a directional search: `major` grows in the direction of the search,
/// `minor` runs across it.
pub type Span = (i32, i32);

/// Where a rectangle of the search's axes is on screen, for `direction`; `mirrored` flips the
/// minor axis.
pub fn on_screen(direction: FocusDirection, major: Span, minor: Span, mirrored: bool) -> Bounds {
    let flip = |(start, end): Span| (1000 - end, 1000 - start);
    let minor = if mirrored { flip(minor) } else { minor };
    match direction {
        Right => ltrb(major.0, minor.0, major.1, minor.1),
        Left => ltrb(flip(major).0, minor.0, flip(major).1, minor.1),
        Down => ltrb(minor.0, major.0, minor.1, major.1),
        Up => ltrb(minor.0, flip(major).0, minor.1, flip(major).1),
        _ => unreachable!("a directional search"),
    }
}

/// The four directions of a directional search.
pub const DIRECTIONS: [FocusDirection; 4] = [Left, Right, Up, Down];

/// A content, the name it starts focused on, a move, and what it gives.
pub struct Move {
    pub what: &'static str,
    pub content: fn(Scene),
    pub from: &'static str,
    pub direction: FocusDirection,
    pub moved: bool,
    pub to: &'static str,
}

/// Each row's move, with what it answers and who has the focus after it.
pub fn check_moves(rows: &[Move]) {
    for row in rows {
        let scene = scene(row.content);
        assert!(scene.request(row.from), "{}: starting from {}", row.what, row.from);
        assert_eq!(scene.move_focus(row.direction), row.moved, "{}: the answer", row.what);
        assert_eq!(scene.focused(), [row.to], "{}", row.what);
    }
}

// ---- What a test does and asks --------------------------------------------------------------------

impl Scene {
    pub fn idle(self) {
        self.0.host.borrow_mut().as_mut().expect("composed").settle(100);
    }

    /// `action` with the host, not in a pass.
    pub fn with_host<R>(self, action: impl FnOnce(&mut UiHost) -> R) -> R {
        action(self.0.host.borrow_mut().as_mut().expect("composed"))
    }

    /// The focus manager of the content.
    pub fn manager(self) -> FocusManager {
        self.0.manager.borrow().clone().expect("composed")
    }

    /// Requests the focus for the requester called `name`: whether it took it.
    pub fn request(self, name: &'static str) -> bool {
        self.idle();
        let result = self.requester(name).request_focus();
        self.idle();
        result
    }

    /// Captures the focus by the requester called `name`: whether it did.
    pub fn capture(self, name: &'static str) -> bool {
        self.idle();
        let result = self.requester(name).capture_focus();
        self.idle();
        result
    }

    /// Frees the focus by the requester called `name`: whether it did.
    pub fn free(self, name: &'static str) -> bool {
        self.idle();
        let result = self.requester(name).free_focus();
        self.idle();
        result
    }

    /// `FocusManager::move_focus`: whether it moved.
    pub fn move_focus(self, direction: FocusDirection) -> bool {
        self.idle();
        let result = self.manager().move_focus(direction);
        self.idle();
        result
    }

    /// A search with wrapping or without, the owner's internal flag in Compose.
    pub fn move_focus_wrapping(self, direction: FocusDirection, wrap: bool) -> bool {
        self.idle();
        let (moved, actions) = self.with_host(|host| {
            host.with_tree(|tree| {
                if wrap {
                    tree.move_focus_in(direction)
                } else {
                    tree.move_focus_without_wrapping(direction)
                }
            })
        });
        for action in actions {
            action.run();
        }
        self.idle();
        moved
    }

    /// `FocusManager::clear_focus`.
    pub fn clear(self, force: bool) {
        self.idle();
        self.manager().clear_focus(force);
        self.idle();
    }

    /// Writes the flag called `name` and settles.
    pub fn set(self, flag: &'static str, value: bool) {
        self.flag(flag).set(value);
        self.idle();
    }

    /// The last state heard under `name`, `Inactive` before any.
    pub fn state(self, name: &'static str) -> FocusState {
        self.idle();
        self.0.last.borrow().get(name).copied().unwrap_or(Inactive)
    }

    /// The last states heard under `names`.
    pub fn states<const N: usize>(self, names: [&'static str; N]) -> [FocusState; N] {
        names.map(|name| self.state(name))
    }

    /// Whether anything has the focus: the root's `ActiveParent`.
    pub fn root_has_focus(self) -> bool {
        self.idle();
        self.with_host(|host| host.with_tree(|tree| tree.focused().is_some()))
    }

    /// The names whose last state is focused, sorted.
    pub fn focused(self) -> Vec<&'static str> {
        self.idle();
        let last = self.0.last.borrow();
        let mut names: Vec<_> =
            last.iter().filter(|(_, state)| state.is_focused()).map(|(name, _)| *name).collect();
        names.sort();
        names
    }

    /// What `on_focus_changed` said under `name`, since the last `forget`.
    pub fn heard(self, name: &'static str) -> Vec<FocusState> {
        self.idle();
        self.0.heard.borrow().get(name).cloned().unwrap_or_default()
    }

    /// What `on_focus_event` said under `name`, since the last `forget`.
    pub fn event_states(self, name: &'static str) -> Vec<FocusState> {
        self.idle();
        self.0.events.borrow().get(name).cloned().unwrap_or_default()
    }

    /// What was said to the log, since the last `forget`.
    pub fn said(self) -> Vec<String> {
        self.idle();
        self.0.log.borrow().clone()
    }

    /// Forgets what was heard and said, each name's last state staying.
    pub fn forget(self) {
        self.idle();
        self.0.heard.borrow_mut().clear();
        self.0.events.borrow_mut().clear();
        self.0.log.borrow_mut().clear();
    }
}

/// A plain value a test and its content's callbacks share.
pub fn cell<T: 'static>(value: T) -> &'static Cell<T> {
    Box::leak(Box::new(Cell::new(value)))
}

/// A state a test and its content share.
pub fn shared<T: PartialEq + 'static>(value: T) -> MutableState<T> {
    Box::leak(Box::new(mutable_state_of(value))).handle()
}
