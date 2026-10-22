//! `NavHost`: the top entry of a `NavController`'s back stack, as Navigation Compose's.

use std::cell::RefCell;
use std::rc::Rc;

use compose_animation::{AnimatedContent, EnterTransition, ExitTransition};
use compose_runtime::{
    Callback, ComposeArg, SaveableStateHolder, SideEffect, composable, remember, remember_keyed,
    remember_saveable_state_holder,
};
use compose_ui::{
    Alignment, BackHandler, Constraints, IntSize, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier,
};

use crate::controller::{NavBackStackEntry, NavController};

/// Shows `content` for the destination of `nav_controller`'s top entry, as Navigation
/// Compose's `NavHost`: a `match` on the app's destination type takes the place of the graph
/// builder.
///
/// - Each entry's content is in a `SaveableStateProvider` keyed by its id: what it
///   `remember_saveable`d (its lists' scroll positions included) is kept while the entry is on
///   the stack, given back when it shows again, and removed when it's popped. `remember`ed
///   values go when another entry shows, as in Compose.
/// - Back pops the stack while there's an entry under the top one (a `BackHandler` composed
///   before the content, so the content's own handlers go first); at the start destination,
///   back leaves the app.
/// - `enter_transition` and `exit_transition` animate the change of entry (`fade_in()` and
///   `fade_out()`, Compose's default, for instance); by default it changes at once.
///
/// ```ignore
/// let nav_controller = remember_nav_controller(Screen::Inbox);
/// NavHost!(nav_controller, move |screen: Screen| match screen {
///     Screen::Inbox => Inbox!(on_open = move |id| nav_controller.navigate(Screen::Message(id))),
///     Screen::Message(id) => MessageScreen!(id),
/// });
/// ```
#[composable]
#[allow(non_snake_case)]
pub fn NavHost<D: Clone + PartialEq + 'static>(
    nav_controller: NavController<D>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Alignment::TOP_START)] content_alignment: Alignment,
    #[default(crate::EnterTransition::NONE)] enter_transition: EnterTransition,
    #[default(crate::ExitTransition::NONE)] exit_transition: ExitTransition,
    content: impl Fn(D) + 'static,
) {
    let holder = remember_saveable_state_holder();
    let back_stack = nav_controller.current_back_stack();
    BackHandler(
        back_stack.len() > 1,
        compose_runtime::callback(move || {
            nav_controller.pop_back_stack();
        }),
    );
    // The state of entries popped since the last pass goes (an entry popped while it fades out
    // keeps its state until it's gone, and leaves none behind).
    let ids: Vec<u64> = back_stack.iter().map(|entry| entry.id).collect();
    let kept = remember(|| Rc::new(RefCell::new(Vec::<u64>::new())));
    SideEffect(move || {
        let mut kept = kept.borrow_mut();
        for id in kept.iter().filter(|id| !ids.contains(id)) {
            holder.remove_state(*id);
        }
        *kept = ids;
    });
    let entry = back_stack.last().expect("a back stack is never empty").clone();
    if enter_transition == EnterTransition::NONE && exit_transition == ExitTransition::NONE {
        let policy = remember_keyed(AlignmentKey(content_alignment), || {
            Rc::new(EntryLayout { alignment: content_alignment }) as Rc<dyn MeasurePolicy>
        });
        Layout(
            modifier,
            policy,
            compose_runtime::callback(move || EntryContent(holder, entry.clone(), &content)),
        );
    } else {
        AnimatedContent(
            entry,
            modifier,
            compose_runtime::callback(move |_, _| {
                enter_transition.together_with(exit_transition).without_size_transform()
            }),
            content_alignment,
            "NavHost",
            compose_runtime::callback(move |entry: NavBackStackEntry<D>| {
                EntryContent(holder, entry, &content)
            }),
        );
    }
}

/// An entry's content, with its saveable state.
#[allow(non_snake_case)]
fn EntryContent<D: Clone + 'static>(
    holder: SaveableStateHolder,
    entry: NavBackStackEntry<D>,
    content: &Callback<dyn Fn(D)>,
) {
    holder.SaveableStateProvider(entry.id, || content(entry.destination));
}

#[derive(Clone, Copy, PartialEq)]
struct AlignmentKey(Alignment);

impl ComposeArg for AlignmentKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The entry, measured with the host's constraints and aligned in the host's size, as
/// `AnimatedContent`'s stack.
struct EntryLayout {
    alignment: Alignment,
}

impl MeasurePolicy for EntryLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let placeables: Vec<_> =
            measurables.iter().map(|&measurable| scope.measure(measurable, constraints)).collect();
        let size = placeables.iter().fold(
            IntSize::new(constraints.min_width, constraints.min_height),
            |size, placeable| {
                IntSize::new(size.width.max(placeable.width), size.height.max(placeable.height))
            },
        );
        for placeable in &placeables {
            let at = self.alignment.align(placeable.size(), size, scope.layout_direction());
            scope.place_at(placeable, at);
        }
        scope.layout(size.width, size.height)
    }

    fn debug_name(&self) -> &'static str {
        "NavHost"
    }
}
