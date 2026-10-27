//! `FocusManager` and `SoftwareKeyboardController`, as Compose's `LocalFocusManager` and
//! `LocalSoftwareKeyboardController`: handles application code clears or moves the focus with,
//! and shows or hides the on-screen keyboard with, from a callback or an effect.
//!
//! Both are handles on their tree's requests (as `FocusRequester`'s): what they ask is done once
//! the tree is free, after the event that asked or at the next frame. Their locals' values are
//! the composing tree's, so no host provides them.

use std::rc::{Rc, Weak};

use compose_runtime::{ComposeArg, CompositionLocal};

use crate::focus_requester::{FocusRequest, FocusRequests};
use crate::input::Action;
use crate::tree::{NodeId, UiTree};
use crate::unit::Rect;

/// Where `FocusManager::move_focus` moves the focus, as Compose's `FocusDirection`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FocusDirection {
    /// The next focusable in Tab's order, wrapping around.
    Next,
    /// The previous one, as Shift+Tab.
    Previous,
    /// The nearest focusable above the focused one (Compose's two-dimensional search).
    Up,
    Down,
    Left,
    Right,
}

/// Clears and moves the focus, as Compose's `FocusManager`: `LOCAL_FOCUS_MANAGER.current()` in
/// composition, then `clear_focus(false)` or `move_focus(FocusDirection::Next)` from a callback.
#[derive(Clone)]
pub struct FocusManager {
    requests: Weak<FocusRequests>,
}

impl FocusManager {
    /// The composing tree's, outside composition or another kind of tree's a manager that does
    /// nothing.
    fn of_composing_tree() -> FocusManager {
        FocusManager { requests: composing_tree_requests() }
    }

    /// Clears the focus, as `clearFocus(force)`: the focused node and its ancestors hear they
    /// lost it, and a text field's input method goes. `force` is Compose's (clearing past a node
    /// that captured the focus): nothing captures the focus here, so it's always cleared.
    pub fn clear_focus(&self, force: bool) {
        if let Some(requests) = self.requests.upgrade() {
            requests.push(FocusRequest::Clear { force });
        }
    }

    /// Moves the focus in `direction`, as `moveFocus(direction)`: `Next` and `Previous` through
    /// Tab's order; `Up`, `Down`, `Left` and `Right` to the nearest focusable that way, as
    /// Compose's two-dimensional search. With nothing focused, `Next`, `Down` and `Right` start
    /// from the window's top left, `Previous`, `Up` and `Left` from its bottom right.
    ///
    /// The move is made once the tree is free: this returns whether it's asked of a tree (Compose
    /// returns whether the focus moved).
    pub fn move_focus(&self, direction: FocusDirection) -> bool {
        match self.requests.upgrade() {
            Some(requests) => {
                requests.push(FocusRequest::Move(direction));
                true
            }
            None => false,
        }
    }
}

impl ComposeArg for FocusManager {
    fn same(&self, other: &Self) -> bool {
        self.requests.ptr_eq(&other.requests)
    }
}

impl std::fmt::Debug for FocusManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FocusManager")
    }
}

/// The focus manager of the composing tree, as Compose's `LocalFocusManager`.
pub static LOCAL_FOCUS_MANAGER: CompositionLocal<FocusManager> =
    CompositionLocal::new("LocalFocusManager", FocusManager::of_composing_tree);

/// Shows and hides the on-screen keyboard, as Compose's `SoftwareKeyboardController`:
/// `LOCAL_SOFTWARE_KEYBOARD_CONTROLLER.current()` in composition, then `hide()` from a callback
/// (a search field's `on_search`).
///
/// It asks the host's platform (`UiHost::take_software_keyboard_request`): Android shows its
/// keyboard for the focused text field, or hides it; desktop has none. As Compose's, `show`
/// does nothing without a focused text field.
#[derive(Clone)]
pub struct SoftwareKeyboardController {
    requests: Weak<FocusRequests>,
}

impl SoftwareKeyboardController {
    pub fn show(&self) {
        self.request(true);
    }

    pub fn hide(&self) {
        self.request(false);
    }

    fn request(&self, show: bool) {
        if let Some(requests) = self.requests.upgrade() {
            requests.software_keyboard.set(Some(show));
            requests.wake();
        }
    }
}

impl ComposeArg for SoftwareKeyboardController {
    fn same(&self, other: &Self) -> bool {
        self.requests.ptr_eq(&other.requests)
    }
}

impl std::fmt::Debug for SoftwareKeyboardController {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SoftwareKeyboardController")
    }
}

/// The keyboard controller of the composing tree, as Compose's
/// `LocalSoftwareKeyboardController`: `None` where there's no tree (another kind of
/// composition), as Compose's is where there's no keyboard.
pub static LOCAL_SOFTWARE_KEYBOARD_CONTROLLER: CompositionLocal<
    Option<SoftwareKeyboardController>,
> = CompositionLocal::new("LocalSoftwareKeyboardController", || {
    let requests = composing_tree_requests();
    (requests.strong_count() > 0).then_some(SoftwareKeyboardController { requests })
});

/// The requests of the tree composing now: none outside a `UiTree`'s composition.
fn composing_tree_requests() -> Weak<FocusRequests> {
    if !compose_runtime::is_composing() {
        return Weak::new();
    }
    let composer = compose_runtime::platform::current();
    if !composer.applier_is::<UiTree>() {
        return Weak::new();
    }
    composer.with_applier(|tree: &mut UiTree| Rc::downgrade(&tree.focus_requests))
}

impl UiTree {
    /// Does what a `FocusRequester` or the `FocusManager` asked, now that the tree is free.
    pub(crate) fn perform_focus_request(&mut self, request: FocusRequest) -> Vec<Action> {
        match request {
            FocusRequest::Node(node) if self.get(node).is_some() => self.request_focus(node),
            FocusRequest::Node(_) => Vec::new(),
            FocusRequest::Clear { force: _ } => self.clear_focus(),
            FocusRequest::Move(direction) => self.move_focus_in(direction).1,
        }
    }

    /// Moves the focus in `direction`, as Compose's `FocusOwner.moveFocus`. Returns whether it
    /// moved, and the actions focus listeners queued.
    ///
    /// A search from inside a lazy list that finds nothing more inside it asks the list for the
    /// next item beyond its bounds, and is made again once it's laid out (at the next layout,
    /// in the frame), as Compose's `BeyondBoundsLayout`; moves asked meanwhile wait their turn
    /// (D477).
    pub fn move_focus_in(&mut self, direction: FocusDirection) -> (bool, Vec<Action>) {
        if self.has_beyond_bounds_moves() {
            self.beyond_search.moves.push_back(direction);
            return (false, Vec::new());
        }
        let before = self.focused();
        let tab = matches!(direction, FocusDirection::Next | FocusDirection::Previous);
        let tab_target =
            if tab { self.tab_target(direction == FocusDirection::Next) } else { None };
        // The beyond-bounds containers around the focus, innermost first, each a focus group
        // as Compose's lazy layouts: the target inside it, else one more item laid out beyond
        // it (the search made again then), else the next container out (a carousel's row in a
        // list, D483). Then the window.
        let mut container = before.and_then(|focused| self.beyond_bounds_container(focused));
        while let Some((id, index)) = container {
            let inside = if tab {
                tab_target
                    .filter(|&(target, wrapped)| !wrapped && self.is_at_or_below(target, id))
                    .map(|(target, _)| target)
            } else {
                self.two_dimensional_search_in(direction, Some(id))
            };
            if let Some(target) = inside {
                self.end_beyond_bounds();
                let actions = self.focus_found(target);
                return (self.focused() != before, actions);
            }
            if self.lay_out_beyond(id, index, direction) {
                return (false, Vec::new());
            }
            container = self.outer_beyond_bounds_container(id);
        }
        self.end_beyond_bounds();
        let target = if tab {
            tab_target.map(|(target, _)| target)
        } else {
            self.two_dimensional_search_in(direction, None)
        };
        let Some(target) = target else { return (false, Vec::new()) };
        let actions = self.focus_found(target);
        (self.focused() != before, actions)
    }

    /// Where `node`'s focus target is, in the root, as Compose's `focusRect()`: the bounds its
    /// place in the chain gives it (after an `offset` or a `padding` before it).
    fn focus_rect(&mut self, node: NodeId) -> Rect {
        let index = match self.focus {
            Some(focus) if focus.node == node => Some(focus.index as usize),
            _ => self.focus_target_index(node),
        };
        let value = self.node(node);
        let Some(index) = index.filter(|_| !value.layers.is_empty()) else {
            return self.bounds_in_root(node);
        };
        let layer = value.layer_of(index).min(value.layers.len() - 1);
        let size = value.layers[layer].measured;
        let origin = self.layer_origin(node, layer);
        Rect::new(origin.x, origin.y, origin.x + size.width as f32, origin.y + size.height as f32)
    }

    /// The focusable nearest the focused node in `direction`, as Compose's
    /// `TwoDimensionalFocusSearch`: among those beyond it that way, one in its beam (overlapping
    /// it across the direction) beats one out of it, else the nearest by
    /// `13 × major² + minor²` (the distance along the direction and the centers' offset across
    /// it). With nothing focused, from the window's top-left corner (`Down`, `Right`) or its
    /// bottom-right one (`Up`, `Left`).
    ///
    /// `within`: only among the focusables inside it (a lazy list's focus group, D483).
    fn two_dimensional_search_in(
        &mut self,
        direction: FocusDirection,
        within: Option<NodeId>,
    ) -> Option<NodeId> {
        let focused = self.focused();
        let source = match focused {
            Some(node) => self.focus_rect(node),
            None => {
                let root = self.bounds_in_root(self.root_id());
                match direction {
                    FocusDirection::Down | FocusDirection::Right => {
                        Rect::new(root.left, root.top, root.left, root.top)
                    }
                    _ => Rect::new(root.right, root.bottom, root.right, root.bottom),
                }
            }
        };
        let mut best: Option<(NodeId, Rect)> = None;
        for node in self.focus_order() {
            if Some(node) == focused
                || within.is_some_and(|within| !self.is_at_or_below(node, within))
            {
                continue;
            }
            let candidate = self.focus_rect(node);
            if !is_candidate(&source, &candidate, direction) {
                continue;
            }
            let better = match &best {
                None => true,
                Some((_, current)) => {
                    if beam_beats(&source, &candidate, current, direction) {
                        true
                    } else if beam_beats(&source, current, &candidate, direction) {
                        false
                    } else {
                        weighted_distance(&source, &candidate, direction)
                            < weighted_distance(&source, current, direction)
                    }
                }
            };
            if better {
                best = Some((node, candidate));
            }
        }
        best.map(|(node, _)| node)
    }
}

/// Whether `rect` is beyond `source` in `direction`, as Compose's `isCandidate`.
fn is_candidate(source: &Rect, rect: &Rect, direction: FocusDirection) -> bool {
    match direction {
        FocusDirection::Left => {
            (source.right > rect.right || source.left >= rect.right) && source.left > rect.left
        }
        FocusDirection::Right => {
            (source.left < rect.left || source.right <= rect.left) && source.right < rect.right
        }
        FocusDirection::Up => {
            (source.bottom > rect.bottom || source.top >= rect.bottom) && source.top > rect.top
        }
        FocusDirection::Down => {
            (source.top < rect.top || source.bottom <= rect.top) && source.bottom < rect.bottom
        }
        FocusDirection::Next | FocusDirection::Previous => false,
    }
}

fn horizontal(direction: FocusDirection) -> bool {
    matches!(direction, FocusDirection::Left | FocusDirection::Right)
}

/// How far `rect` is from `source` along `direction` (0 when they overlap).
fn major_axis_distance(source: &Rect, rect: &Rect, direction: FocusDirection) -> f32 {
    let distance = match direction {
        FocusDirection::Left => source.left - rect.right,
        FocusDirection::Right => rect.left - source.right,
        FocusDirection::Up => source.top - rect.bottom,
        _ => rect.top - source.bottom,
    };
    distance.max(0.0)
}

/// As Compose's `weightedDistance`: the distance along the direction counts 13 times more.
fn weighted_distance(source: &Rect, rect: &Rect, direction: FocusDirection) -> i64 {
    let major = major_axis_distance(source, rect, direction).abs() as i64;
    let minor = if horizontal(direction) {
        (source.top + source.bottom) / 2.0 - (rect.top + rect.bottom) / 2.0
    } else {
        (source.left + source.right) / 2.0 - (rect.left + rect.right) / 2.0
    }
    .abs() as i64;
    13 * major * major + minor * minor
}

/// Whether `first` beats `second` by being in `source`'s beam, as Compose's `beamBeats`.
fn beam_beats(source: &Rect, first: &Rect, second: &Rect, direction: FocusDirection) -> bool {
    let in_beam = |rect: &Rect| {
        if horizontal(direction) {
            rect.bottom > source.top && rect.top < source.bottom
        } else {
            rect.right > source.left && rect.left < source.right
        }
    };
    if in_beam(second) || !in_beam(first) {
        return false;
    }
    let beyond = match direction {
        FocusDirection::Left => source.left >= second.right,
        FocusDirection::Right => source.right <= second.left,
        FocusDirection::Up => source.top >= second.bottom,
        _ => source.bottom <= second.top,
    };
    if !beyond || horizontal(direction) {
        return true;
    }
    // Vertically, a beam candidate beats one out of it unless that one's nearer edge comes
    // before its far edge.
    let far_edge = match direction {
        FocusDirection::Up => source.top - second.top,
        _ => second.bottom - source.bottom,
    }
    .max(1.0);
    major_axis_distance(source, first, direction) < far_edge
}
