//! `Crossfade`, as Compose's: content for a state that fades into the content for the next.

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::{
    SideEffect, composable, key, mutable_state_of, remember, remember_coroutine_scope,
};
use compose_ui::{
    Constraints, IntSize, Layout, Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier,
};

use crate::animatable::Animatable;
use crate::spec::AnimationSpec;

/// One state's content: its alpha animates in, then out when another state follows.
struct Item<T> {
    id: u64,
    state: T,
    alpha: Animatable<f32>,
}

struct Items<T> {
    items: Vec<Item<T>>,
    next_id: u64,
}

/// Stacks its children at the top start, as large as the largest.
struct StackLayout;

impl MeasurePolicy for StackLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let loose = constraints.loosen();
        let placeables: Vec<_> =
            measurables.iter().map(|&measurable| scope.measure(measurable, loose)).collect();
        let size = placeables.iter().fold(
            IntSize::new(constraints.min_width, constraints.min_height),
            |size, placeable| {
                IntSize::new(size.width.max(placeable.width), size.height.max(placeable.height))
            },
        );
        for placeable in &placeables {
            scope.place_relative(placeable, 0, 0);
        }
        scope.layout(size.width, size.height)
    }

    fn debug_name(&self) -> &'static str {
        "Crossfade"
    }
}

/// Shows `content(target_state)`; when the state changes, the new content fades in over the old
/// one, which fades out and leaves, along `animation_spec`, as Compose's `Crossfade`. States are
/// told apart with `==`.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn Crossfade<T: Clone + PartialEq + 'static>(
    target_state: T,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::AnimationSpec::Tween(crate::tween(crate::DEFAULT_DURATION_MILLIS)))]
    animation_spec: impl Into<AnimationSpec>,
    #[default("Crossfade")] label: &'static str,
    content: impl Fn(T) + 'static,
) {
    let _ = label;
    let spec = animation_spec.into();
    let items = remember(|| {
        Rc::new(RefCell::new(Items {
            items: vec![Item { id: 0, state: target_state.clone(), alpha: Animatable::new(1.0) }],
            next_id: 1,
        }))
    });
    // Bumped when a faded-out item leaves: the stack recomposes without it.
    // Read by the stack's content only: the call site doesn't recompose for the stack's own
    // changes (it would compare its target again: a state unequal to itself, holding a NaN,
    // would be added at each pass).
    let removed = remember(|| mutable_state_of(0u64));
    let scope = remember_coroutine_scope();
    {
        let items = items.clone();
        let target = target_state.clone();
        SideEffect(move || {
            let mut all = items.borrow_mut();
            if all.items.last().is_some_and(|item| item.state == target) {
                return;
            }
            // A state still shown (fading out) fades back in, its content kept, as Compose's;
            // another one is added.
            let alpha = match all.items.iter().position(|item| item.state == target) {
                Some(index) => {
                    let item = all.items.remove(index);
                    let alpha = item.alpha.clone();
                    all.items.push(item);
                    alpha
                }
                None => {
                    let id = all.next_id;
                    all.next_id += 1;
                    let alpha = Animatable::new(0.0);
                    all.items.push(Item { id, state: target, alpha: alpha.clone() });
                    alpha
                }
            };
            // The new one in, the others out (and gone once out).
            scope.launch(async move {
                alpha.animate_to(1.0, spec).await;
            });
            for item in &all.items[..all.items.len() - 1] {
                let (alpha, id, items) = (item.alpha.clone(), item.id, items.clone());
                scope.launch(async move {
                    let result = alpha.animate_to(0.0, spec).await;
                    if result.end_reason == crate::AnimationEndReason::Finished {
                        items.borrow_mut().items.retain(|item| item.id != id);
                        removed.update(|count| count + 1);
                    }
                });
            }
            drop(all);
            // The new item shows at the next pass.
            removed.update(|count| count + 1);
        });
    }
    let content = Rc::new(content);
    let policy = remember(|| Rc::new(StackLayout) as Rc<dyn MeasurePolicy>);
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            removed.get();
            let shown: Vec<(u64, T, Animatable<f32>)> = items
                .borrow()
                .items
                .iter()
                .map(|item| (item.id, item.state.clone(), item.alpha.clone()))
                .collect();
            for (id, state, alpha) in &shown {
                let content = content.clone();
                let state = state.clone();
                let alpha = alpha.clone();
                key(*id, move || {
                    let layer = remember(|| {
                        Modifier.graphics_layer(move |layer| layer.alpha = alpha.value())
                    });
                    Layout(
                        layer,
                        remember(|| Rc::new(StackLayout) as Rc<dyn MeasurePolicy>),
                        compose_runtime::callback(move || content(state.clone())),
                    );
                });
            }
        }),
    );
}
