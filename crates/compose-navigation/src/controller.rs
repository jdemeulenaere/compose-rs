//! `NavController`: the back stack of an app's destinations, as Navigation Compose's.

use std::fmt;
use std::hash::{BuildHasher, Hasher};

use compose_runtime::{
    ComposeArg, MutableState, ObjectHandle, Owned, OwnedState, Saveable, SavedValue, Saver,
    mutable_state_of, remember_saveable_with, saver,
};

/// A destination on the back stack, as Navigation Compose's `NavBackStackEntry`: the
/// destination, and an id of its own (two entries of one destination are two entries), which
/// keys its saveable state. Ids are random, as Navigation Compose's UUIDs: unique across
/// controllers and processes, so a controller replacing another in a `NavHost`, or a stack
/// started again when the saved one couldn't be restored, never takes an old entry's state.
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
}

/// A new entry's id: random (the thread's random hasher keys, a new pair for each call, as
/// `RandomState` makes them), as Navigation Compose's entries take a random UUID.
fn new_entry_id() -> u64 {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u8(0);
    hasher.finish()
}

/// What a read of a stale controller panics with: there's no destination to give.
#[cold]
#[track_caller]
fn stale_read() -> ! {
    panic!(
        "NavController: the current destination was read after the controller's owner (its \
         `remember_nav_controller` slot, or the `Owned` that `NavController::new` returned) \
         was dropped; there's no destination to give (D291)"
    )
}

/// The back stack of an app's destinations, as Navigation Compose's `NavController`: its
/// [`NavHost`](crate::NavHost) shows the top entry. Destinations are values of the app's own
/// type (an enum, typically), compared with `==`; the stack always has its start destination
/// at least.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_nav_controller`] keeps it in the composition, saved across the host's recreation.
/// Once the owner went, it does nothing, with a warning in debug builds: navigating and popping
/// change nothing, the stack reads as empty and `previous_back_stack_entry` as `None`; but
/// `current_destination` and `current_back_stack_entry`, having nothing to give, panic.
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
        NavController::with_back_stack(vec![NavBackStackEntry {
            id: new_entry_id(),
            destination: start_destination,
        }])
    }

    #[track_caller]
    fn with_back_stack(entries: Vec<NavBackStackEntry<D>>) -> Owned<Self> {
        let inner = ControllerInner { back_stack: mutable_state_of(entries) };
        let back_stack = inner.back_stack.handle();
        Owned::new(inner, move |object| NavController { back_stack, object })
    }

    /// `read` of the stack (a state read), or `None` once the owner went (D240: a warning in
    /// debug builds, naming the caller).
    #[track_caller]
    fn read<R>(&self, read: impl FnOnce(&[NavBackStackEntry<D>]) -> R) -> Option<R> {
        self.object.get()?;
        Some(self.back_stack.with(|stack| read(stack)))
    }

    /// The entries, the start destination's first and the one shown last, as
    /// `currentBackStack` (a state read). Empty once the owner went.
    #[track_caller]
    pub fn current_back_stack(&self) -> Vec<NavBackStackEntry<D>> {
        self.read(<[_]>::to_vec).unwrap_or_default()
    }

    /// The entry shown, as `currentBackStackEntry` (a state read).
    ///
    /// # Panics
    ///
    /// Once the controller's owner went: there's no entry to give.
    #[track_caller]
    pub fn current_back_stack_entry(&self) -> NavBackStackEntry<D> {
        self.read(|stack| stack.last().cloned()).flatten().unwrap_or_else(|| stale_read())
    }

    /// The destination shown (a state read: what reads it recomposes when it changes).
    ///
    /// # Panics
    ///
    /// Once the controller's owner went: there's no destination to give.
    #[track_caller]
    pub fn current_destination(&self) -> D {
        self.read(|stack| stack.last().map(|entry| entry.destination.clone()))
            .flatten()
            .unwrap_or_else(|| stale_read())
    }

    /// Whether the entry `id` is shown (a state read).
    pub(crate) fn is_top(&self, id: u64) -> bool {
        self.read(|stack| stack.last().is_some_and(|entry| entry.id == id)).unwrap_or(false)
    }

    /// The entry under the one shown, if any, as `previousBackStackEntry` (a state read).
    /// `None` once the owner went.
    #[track_caller]
    pub fn previous_back_stack_entry(&self) -> Option<NavBackStackEntry<D>> {
        self.read(|stack| stack.len().checked_sub(2).map(|index| stack[index].clone())).flatten()
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
        if self.object.get().is_none() {
            return;
        }
        let mut stack = self.back_stack.get();
        if let Some(pop_up_to) = &options.pop_up_to
            && let Some(index) = stack.iter().rposition(|entry| entry.destination == *pop_up_to)
        {
            stack.truncate(if options.pop_up_to_inclusive { index } else { index + 1 });
        }
        let on_top = stack.last().is_some_and(|entry| entry.destination == destination);
        if !(options.launch_single_top && on_top) {
            stack.push(NavBackStackEntry { id: new_entry_id(), destination });
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
    /// `NavControllerSaver`. A stack that can't be restored (a destination of another version of
    /// the app) starts again at the start destination.
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
                Some(SavedValue::List(entries))
            },
            |saved| {
                let SavedValue::List(saved_entries) = saved else { return None };
                let mut entries = Vec::with_capacity(saved_entries.len());
                for entry in saved_entries {
                    let SavedValue::List(pair) = entry else { return None };
                    let [SavedValue::Int(id), destination] = pair.as_slice() else { return None };
                    entries.push(NavBackStackEntry {
                        id: *id as u64,
                        destination: D::restore(destination)?,
                    });
                }
                // No entry: start again.
                if entries.is_empty() {
                    return None;
                }
                Some(NavController::with_back_stack(entries))
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
