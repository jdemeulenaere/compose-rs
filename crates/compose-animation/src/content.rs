//! `AnimatedContent`, as Compose's: content for a state that changes with an animation, the
//! new content entering as the old one exits, the container's size animating between theirs.

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::{composable, key, remember};
use compose_ui::{
    Alignment, Constraints, IntSize, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier,
};

use crate::content_size::AnimationModifiers;
use crate::spec::tween;
use crate::visibility::{
    AnimatedEnterExit, EnterTransition, ExitTransition, fade_in_with, fade_out_with, scale_in_with,
};

/// How one state's content gives way to the next's, as Compose's `ContentTransform`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ContentTransform {
    pub target_content_enter: EnterTransition,
    pub initial_content_exit: ExitTransition,
    /// Whether the container's size animates between the contents' (as `SizeTransform`).
    pub size_transform: bool,
}

impl EnterTransition {
    /// This enter with `exit` for the content leaving, as `togetherWith`.
    pub fn together_with(self, exit: ExitTransition) -> ContentTransform {
        ContentTransform {
            target_content_enter: self,
            initial_content_exit: exit,
            size_transform: true,
        }
    }
}

impl ContentTransform {
    /// Without animating the container's size, as `using(SizeTransform)` turned off.
    pub fn without_size_transform(self) -> ContentTransform {
        ContentTransform { size_transform: false, ..self }
    }
}

/// Compose's default `AnimatedContent` transition: the new content fading (220 ms, after
/// 90 ms) and scaling in from 92 %, the old one fading out (90 ms), the size animating.
pub fn default_content_transform() -> ContentTransform {
    (fade_in_with(tween(220).delay(90), 0.0) + scale_in_with(tween(220).delay(90), 0.92))
        .together_with(fade_out_with(tween(90), 0.0))
}

/// A state shown, with the enter it was given when it became the target and the exit it was
/// given when it stopped being it, as Compose's.
#[derive(Clone)]
struct ShownState<T> {
    id: u64,
    state: T,
    enter: EnterTransition,
    exit: ExitTransition,
}

/// The states shown: the target last.
struct Shown<T> {
    states: Vec<ShownState<T>>,
    next_id: u64,
}

/// Shows `content(target_state)`; when the state changes, the new content enters as the old
/// one exits (`transition_spec(initial, target)`), stacked and aligned by
/// `content_alignment`, as Compose's `AnimatedContent`. States are told apart with `==`. The
/// last three states' contents are kept while they animate (older ones leave at once).
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn AnimatedContent<T: Clone + PartialEq + 'static>(
    target_state: T,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(|_, _| crate::default_content_transform())] transition_spec: impl Fn(
        T,
        T,
    )
        -> ContentTransform
    + 'static,
    #[default(crate::Alignment::TOP_START)] content_alignment: Alignment,
    #[default("AnimatedContent")] label: &'static str,
    content: impl Fn(T) + 'static,
) {
    let _ = label;
    let shown = remember(|| {
        let first = default_content_transform();
        Rc::new(RefCell::new(Shown {
            states: vec![ShownState {
                id: 0,
                state: target_state.clone(),
                enter: first.target_content_enter,
                exit: first.initial_content_exit,
            }],
            next_id: 1,
        }))
    });
    // The transition of the last change, as `transition_spec` gave it.
    let transform = remember(|| Rc::new(std::cell::Cell::new(None::<ContentTransform>)));
    {
        let mut all = shown.borrow_mut();
        let last = all.states.last().map(|shown| shown.state.clone());
        if last.as_ref() != Some(&target_state) {
            let change = match &last {
                Some(initial) => transition_spec(initial.clone(), target_state.clone()),
                None => default_content_transform(),
            };
            transform.set(Some(change));
            // The content leaving takes this change's exit.
            if let Some(leaving) = all.states.last_mut() {
                leaving.exit = change.initial_content_exit;
            }
            // A state still shown comes back; another one is added. Either enters with this
            // change's enter.
            let mut entering = match all.states.iter().position(|shown| shown.state == target_state)
            {
                Some(index) => all.states.remove(index),
                None => {
                    let id = all.next_id;
                    all.next_id += 1;
                    ShownState {
                        id,
                        state: target_state.clone(),
                        enter: change.target_content_enter,
                        exit: change.initial_content_exit,
                    }
                }
            };
            entering.enter = change.target_content_enter;
            all.states.push(entering);
            let excess = all.states.len().saturating_sub(3);
            all.states.drain(..excess);
        }
    }
    let transform = transform.get().unwrap_or_else(default_content_transform);
    let states = shown.borrow().states.clone();
    let content = Rc::new(content);
    let largest = !transform.size_transform;
    let policy = compose_runtime::remember_keyed(StackKey(content_alignment, largest), || {
        Rc::new(AlignedStack { alignment: content_alignment, largest }) as Rc<dyn MeasurePolicy>
    });
    let modifier = if transform.size_transform {
        modifier.clip(compose_ui::RECTANGLE_SHAPE).animate_content_size()
    } else {
        modifier
    };
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            let target = states.len() - 1;
            for (index, shown) in states.iter().enumerate() {
                let (content, state) = (content.clone(), shown.state.clone());
                let (enter, exit) = (shown.enter, shown.exit);
                let visible = index == target;
                // The first state's content is there at once; the others enter.
                let initially_visible = shown.id == 0;
                key(shown.id, move || {
                    let (content, state) = (content.clone(), state.clone());
                    AnimatedEnterExit(
                        visible,
                        initially_visible,
                        Modifier,
                        enter,
                        exit,
                        compose_runtime::callback(move || content(state.clone())),
                    );
                });
            }
        }),
    );
}

#[derive(Clone, Copy, PartialEq)]
struct StackKey(Alignment, bool);

impl compose_runtime::ComposeArg for StackKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The contents stacked, measured with the container's constraints, aligned in the target's
/// size (what a size transform animates) or, without one, the largest of them.
struct AlignedStack {
    alignment: Alignment,
    largest: bool,
}

impl MeasurePolicy for AlignedStack {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let placeables: Vec<_> =
            measurables.iter().map(|&measurable| scope.measure(measurable, constraints)).collect();
        let empty = IntSize::new(constraints.min_width, constraints.min_height);
        let size = if self.largest {
            placeables.iter().fold(empty, |size, placeable| {
                IntSize::new(size.width.max(placeable.width), size.height.max(placeable.height))
            })
        } else {
            // As large as the content entering (the last), as Compose's target size.
            placeables.last().map_or(empty, |last| IntSize::new(last.width, last.height))
        };
        for placeable in &placeables {
            let at = self.alignment.align(placeable.size(), size, scope.layout_direction());
            scope.place_at(placeable, at);
        }
        scope.layout(size.width, size.height)
    }

    fn debug_name(&self) -> &'static str {
        "AnimatedContent"
    }
}
