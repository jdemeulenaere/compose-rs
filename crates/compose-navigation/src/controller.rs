//! `NavController`: the back stack of an app's destinations, as Navigation Compose's.

use std::cell::Cell;
use std::fmt;

use compose_runtime::{
    ComposeArg, MutableState, ObjectHandle, Owned, OwnedState, Saveable, SavedValue, Saver,
    mutable_state_of, remember_saveable_with, saver,
};

/// A destination on the back stack, as Navigation Compose's `NavBackStackEntry`: the
/// destination, and an id of its own (two entries of one destination are two entries), which
/// keys its saveable state.
#[derive(Clone, PartialEq, Debug)]
pub struct NavBackStackEntry<D> {
    pub id: u64,
    pub destination: D,
}

/// How [`NavController::navigate_with`] navigates, as Navigation Compose's `NavOptions`.
#[derive(Clone, PartialEq, Debug)]
pub struct NavOptions<D> {
    /// Don't add the destination if it's already on top (equal to the top entry's), as
    /// `launchSingleTop`.
    pub launch_single_top: bool,
    /// Pops the entries above the topmost one of this destination first, as `popUpTo`;
    /// nothing is popped if none is on the stack.
    pub pop_up_to: Option<D>,
    /// Pops that entry too, as `popUpTo(…) { inclusive = true }`.
    pub pop_up_to_inclusive: bool,
}

impl<D> Default for NavOptions<D> {
    fn default() -> NavOptions<D> {
        NavOptions { launch_single_top: false, pop_up_to: None, pop_up_to_inclusive: false }
    }
}

/// What a controller's owner keeps.
struct ControllerInner<D: 'static> {
    back_stack: OwnedState<Vec<NavBackStackEntry<D>>>,
    next_id: Cell<u64>,
}

/// The back stack of an app's destinations, as Navigation Compose's `NavController`: its
/// [`NavHost`](crate::NavHost) shows the top entry. Destinations are values of the app's own
/// type (an enum, typically), compared with `==`; the stack always has its start destination
/// at least.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_nav_controller`] keeps it in the composition, saved across the host's recreation.
/// Navigating after the owner went does nothing; reading the stack then panics (there's no
/// destination to give).
pub struct NavController<D: 'static> {
    back_stack: MutableState<Vec<NavBackStackEntry<D>>>,
    object: ObjectHandle<ControllerInner<D>>,
}

impl<D: 'static> Clone for NavController<D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<D: 'static> Copy for NavController<D> {}

impl<D: 'static> PartialEq for NavController<D> {
    fn eq(&self, other: &Self) -> bool {
        self.object == other.object
    }
}

impl<D: 'static> Eq for NavController<D> {}

impl<D: 'static> fmt::Debug for NavController<D> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "NavController({:?})", self.object)
    }
}

impl<D: 'static> ComposeArg for NavController<D> {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl<D: Clone + PartialEq + 'static> NavController<D> {
    /// A controller whose stack holds `start_destination`.
    #[track_caller]
    pub fn new(start_destination: D) -> Owned<NavController<D>> {
        NavController::with_back_stack(
            vec![NavBackStackEntry { id: 0, destination: start_destination }],
            1,
        )
    }

    #[track_caller]
    fn with_back_stack(entries: Vec<NavBackStackEntry<D>>, next_id: u64) -> Owned<Self> {
        let inner =
            ControllerInner { back_stack: mutable_state_of(entries), next_id: Cell::new(next_id) };
        let back_stack = inner.back_stack.handle();
        Owned::new(inner, move |object| NavController { back_stack, object })
    }

    /// The entries, the start destination's first and the one shown last, as
    /// `currentBackStack` (a state read).
    #[track_caller]
    pub fn current_back_stack(&self) -> Vec<NavBackStackEntry<D>> {
        self.back_stack.get()
    }

    /// The entry shown, as `currentBackStackEntry` (a state read).
    #[track_caller]
    pub fn current_back_stack_entry(&self) -> NavBackStackEntry<D> {
        self.back_stack.with(|stack| stack.last().expect("a back stack is never empty").clone())
    }

    /// The destination shown (a state read: what reads it recomposes when it changes).
    #[track_caller]
    pub fn current_destination(&self) -> D {
        self.back_stack
            .with(|stack| stack.last().expect("a back stack is never empty").destination.clone())
    }

    /// The entry under the one shown, if any, as `previousBackStackEntry` (a state read).
    #[track_caller]
    pub fn previous_back_stack_entry(&self) -> Option<NavBackStackEntry<D>> {
        self.back_stack.with(|stack| stack.len().checked_sub(2).map(|index| stack[index].clone()))
    }

    /// Shows `destination`, on top of the stack, as `navigate(route)`.
    #[track_caller]
    pub fn navigate(&self, destination: D) {
        self.navigate_with(destination, NavOptions::default());
    }

    /// Shows `destination` as `options` say, as `navigate(route) { … }`: first pops up to
    /// `pop_up_to` (if it's on the stack), then pushes `destination` unless
    /// `launch_single_top` and it's already on top.
    #[track_caller]
    pub fn navigate_with(&self, destination: D, options: NavOptions<D>) {
        let Some(inner) = self.object.get() else { return };
        let mut stack = self.back_stack.get();
        if let Some(pop_up_to) = &options.pop_up_to
            && let Some(index) = stack.iter().rposition(|entry| entry.destination == *pop_up_to)
        {
            stack.truncate(if options.pop_up_to_inclusive { index } else { index + 1 });
        }
        let on_top = stack.last().is_some_and(|entry| entry.destination == destination);
        if !(options.launch_single_top && on_top) {
            let id = inner.next_id.get();
            inner.next_id.set(id + 1);
            stack.push(NavBackStackEntry { id, destination });
        }
        self.back_stack.set(stack);
    }

    /// Pops the entry shown, showing the one under it, as `popBackStack()`. Returns whether
    /// there was one under it: the start destination isn't popped.
    #[track_caller]
    pub fn pop_back_stack(&self) -> bool {
        if self.object.get().is_none() {
            return false;
        }
        let mut stack = self.back_stack.get();
        if stack.len() < 2 {
            return false;
        }
        stack.pop();
        self.back_stack.set(stack);
        true
    }

    /// Pops the entries above the topmost one of `destination` (and it too if `inclusive`), as
    /// `popBackStack(route, inclusive)`. Returns whether any was: nothing is popped if
    /// `destination` isn't on the stack, or if it would leave the stack empty.
    #[track_caller]
    pub fn pop_back_stack_to(&self, destination: &D, inclusive: bool) -> bool {
        if self.object.get().is_none() {
            return false;
        }
        let mut stack = self.back_stack.get();
        let Some(index) = stack.iter().rposition(|entry| entry.destination == *destination) else {
            return false;
        };
        let length = if inclusive { index } else { index + 1 };
        if length == 0 || length == stack.len() {
            return false;
        }
        stack.truncate(length);
        self.back_stack.set(stack);
        true
    }
}

impl<D: Saveable + Clone + PartialEq + 'static> NavController<D> {
    /// Saves the back stack (each entry's id and destination), as Navigation Compose's
    /// `NavControllerSaver`.
    pub fn saver() -> impl Saver<Owned<NavController<D>>> {
        saver(
            |controller: &Owned<NavController<D>>| {
                let inner = controller.object.try_get()?;
                let entries = inner.back_stack.with(|stack| {
                    stack
                        .iter()
                        .map(|entry| {
                            SavedValue::List(vec![
                                SavedValue::Int(entry.id as i64),
                                entry.destination.save(),
                            ])
                        })
                        .collect()
                });
                Some(SavedValue::List(vec![
                    SavedValue::Int(inner.next_id.get() as i64),
                    SavedValue::List(entries),
                ]))
            },
            |saved| {
                let SavedValue::List(parts) = saved else { return None };
                let [SavedValue::Int(next_id), SavedValue::List(saved_entries)] = parts.as_slice()
                else {
                    return None;
                };
                let mut entries = Vec::with_capacity(saved_entries.len());
                for entry in saved_entries {
                    let SavedValue::List(pair) = entry else { return None };
                    let [SavedValue::Int(id), destination] = pair.as_slice() else { return None };
                    entries.push(NavBackStackEntry {
                        id: *id as u64,
                        destination: D::restore(destination)?,
                    });
                }
                // A destination of another version of the app, or no entry: start again.
                if entries.is_empty() {
                    return None;
                }
                Some(NavController::with_back_stack(entries, *next_id as u64))
            },
        )
    }
}

/// A [`NavController`] whose stack starts at `start_destination`, kept in the composition and
/// saved across the host's recreation (an Android configuration change, the process's death),
/// as Navigation Compose's `rememberNavController()` (which takes the start destination from
/// `NavHost`). Destinations are [`Saveable`] for that.
#[track_caller]
pub fn remember_nav_controller<D: Saveable + Clone + PartialEq + 'static>(
    start_destination: D,
) -> NavController<D> {
    remember_saveable_with(NavController::saver(), || NavController::new(start_destination))
}
