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
    AnimatedVisibility, EnterTransition, ExitTransition, fade_in_with, fade_out_with, scale_in_with,
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

/// The states shown: the target last.
struct Shown<T> {
    states: Vec<(u64, T)>,
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
        Rc::new(RefCell::new(Shown { states: vec![(0, target_state.clone())], next_id: 1 }))
    });
    // The transition of the last change, as `transition_spec` gave it.
    let transform = remember(|| Rc::new(std::cell::Cell::new(None::<ContentTransform>)));
    {
        let mut all = shown.borrow_mut();
        let last = all.states.last().map(|(_, state)| state.clone());
        if last.as_ref() != Some(&target_state) {
            if let Some(initial) = &last {
                transform.set(Some(transition_spec(initial.clone(), target_state.clone())));
            }
            // A state still shown comes back; another one is added.
            match all.states.iter().position(|(_, state)| *state == target_state) {
                Some(index) => {
                    let item = all.states.remove(index);
                    all.states.push(item);
                }
                None => {
                    let id = all.next_id;
                    all.next_id += 1;
                    all.states.push((id, target_state.clone()));
                }
            }
            let excess = all.states.len().saturating_sub(3);
            all.states.drain(..excess);
        }
    }
    let transform = transform.get().unwrap_or_else(default_content_transform);
    let states = shown.borrow().states.clone();
    let content = Rc::new(content);
    let policy = compose_runtime::remember_keyed(AlignmentKey(content_alignment), || {
        Rc::new(AlignedStack { alignment: content_alignment }) as Rc<dyn MeasurePolicy>
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
            for (index, (id, state)) in states.iter().enumerate() {
                let (content, state) = (content.clone(), state.clone());
                let visible = index == target;
                key(*id, move || {
                    let (content, state) = (content.clone(), state.clone());
                    AnimatedVisibility(
                        visible,
                        Modifier,
                        transform.target_content_enter,
                        transform.initial_content_exit,
                        "AnimatedContent",
                        compose_runtime::callback(move || content(state.clone())),
                    );
                });
            }
        }),
    );
}

#[derive(Clone, Copy, PartialEq)]
struct AlignmentKey(Alignment);

impl compose_runtime::ComposeArg for AlignmentKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The contents stacked, aligned in the largest of them.
struct AlignedStack {
    alignment: Alignment,
}

impl MeasurePolicy for AlignedStack {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let loose = constraints.loosen();
        let placeables: Vec<_> =
            measurables.iter().map(|&measurable| scope.measure(measurable, loose)).collect();
        // As large as the content entering (the last), as Compose's target size.
        let size = placeables.last().map_or(
            IntSize::new(constraints.min_width, constraints.min_height),
            |last| {
                IntSize::new(
                    constraints.constrain_width(last.width),
                    constraints.constrain_height(last.height),
                )
            },
        );
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
