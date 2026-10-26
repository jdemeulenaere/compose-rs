//! `Modifier.animate_content_size`, as Compose's `animateContentSize`: a node whose size follows
//! its content's, animated.

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::CoroutineScope;
use compose_ui::{
    Alignment, Constraints, DetachContext, IntSize, LayoutModifierNode, Measurable, MeasureResult,
    MeasureScope, Modifier, ModifierNode, ModifierNodeElement,
};

use crate::animatable::{Animatable, AnimationEndReason};
use crate::spec::AnimationSpec;
use crate::vector::VisibilityThreshold;

/// What hears a size animation that finished: its start and end sizes.
type FinishedListener = Rc<dyn Fn(IntSize, IntSize)>;

/// The size-animating modifiers, on `Modifier`.
pub trait AnimationModifiers {
    /// Animates this node's size when its content's changes, along a spring (stiffness 400, a
    /// pixel's threshold), as `Modifier.animateContentSize()`. The content is laid out at its new
    /// size at once, placed at the top start and clipped to the animated size.
    fn animate_content_size(self) -> Modifier;
    /// The same along `animation_spec`, the content placed by `alignment` in the animated size,
    /// `finished_listener` hearing each animation that finishes (its start and end sizes), as
    /// `Modifier.animateContentSize(animationSpec, alignment, finishedListener)`.
    fn animate_content_size_with(
        self,
        animation_spec: impl Into<AnimationSpec>,
        alignment: Alignment,
        finished_listener: Option<impl Fn(IntSize, IntSize) + 'static>,
    ) -> Modifier;
}

/// `animate_content_size`'s default spring.
fn default_spec() -> AnimationSpec {
    AnimationSpec::Spring(
        crate::spec::spring_with(1.0, crate::Spring::STIFFNESS_MEDIUM_LOW)
            .visibility_threshold(IntSize::VISIBILITY_THRESHOLD),
    )
}

impl AnimationModifiers for Modifier {
    fn animate_content_size(self) -> Modifier {
        self.animate_content_size_with(default_spec(), Alignment::TOP_START, None::<fn(_, _)>)
    }

    fn animate_content_size_with(
        self,
        animation_spec: impl Into<AnimationSpec>,
        alignment: Alignment,
        finished_listener: Option<impl Fn(IntSize, IntSize) + 'static>,
    ) -> Modifier {
        let listener = finished_listener.map(|listener| Rc::new(listener) as FinishedListener);
        self.clip(compose_ui::RECTANGLE_SHAPE).then_custom(ContentSizeElement {
            spec: animation_spec.into(),
            alignment,
            listener,
        })
    }
}

struct ContentSizeElement {
    spec: AnimationSpec,
    alignment: Alignment,
    listener: Option<FinishedListener>,
}

/// The animation, once measured, as Compose's `AnimData`.
struct AnimData {
    animatable: Animatable<IntSize>,
    /// Where the animation running started.
    start_size: Rc<Cell<IntSize>>,
    /// How many animations run (an interrupted one until it sees it was): `isRunning`, read
    /// while measuring without measuring again when it changes (a frame a test runs at the same
    /// time would start the animation a frame early).
    running: Rc<Cell<u32>>,
}

/// Counts an animation running until it ends or is dropped (its scope cancelled).
struct RunningGuard(Rc<Cell<u32>>);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

struct ContentSizeNode {
    spec: AnimationSpec,
    alignment: Alignment,
    listener: Option<FinishedListener>,
    data: Option<AnimData>,
    scope: compose_runtime::Owned<CoroutineScope>,
}

impl ModifierNodeElement for ContentSizeElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(ContentSizeNode {
            spec: self.spec,
            alignment: self.alignment,
            listener: self.listener.clone(),
            data: None,
            scope: CoroutineScope::new(),
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<ContentSizeNode>() {
            node.spec = self.spec;
            node.alignment = self.alignment;
            node.listener = self.listener.clone();
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<ContentSizeElement>().is_some_and(|other| {
            other.spec == self.spec
                && other.alignment == self.alignment
                && match (&other.listener, &self.listener) {
                    (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                    (None, None) => true,
                    _ => false,
                }
        })
    }

    fn name(&self) -> &'static str {
        "animateContentSize"
    }
}

impl ModifierNode for ContentSizeNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }

    fn on_detach(&mut self, _context: &mut DetachContext<'_>) {
        self.scope.cancel();
        // A node attached again (reused) starts a scope of its own.
        self.scope = CoroutineScope::new();
    }

    /// Reused, the new content's size is taken at once, as Compose's `animData = null`.
    fn on_reset(&mut self) {
        self.data = None;
    }
}

impl ContentSizeNode {
    /// Animates to `target_size`, as Compose's `animateTo`: the size to take now.
    fn animate_to(&mut self, target_size: IntSize) -> IntSize {
        let Some(data) = &self.data else {
            let animatable =
                Animatable::new_with(target_size, Some(IntSize::VISIBILITY_THRESHOLD), "Size");
            self.data = Some(AnimData {
                animatable,
                start_size: Rc::new(Cell::new(target_size)),
                running: Rc::new(Cell::new(0)),
            });
            return target_size;
        };
        let current = data.animatable.value();
        let was_interrupted = target_size != current && data.running.get() == 0;
        if target_size != data.animatable.target_value() || was_interrupted {
            data.start_size.set(current);
            let (animatable, start, spec) =
                (data.animatable.clone(), data.start_size.clone(), self.spec);
            let listener = self.listener.clone();
            data.running.set(data.running.get() + 1);
            let guard = RunningGuard(data.running.clone());
            self.scope.launch(async move {
                let _guard = guard;
                let result = animatable.animate_to(target_size, spec).await;
                let finished = result.end_reason == AnimationEndReason::Finished;
                if let Some(listener) = listener.filter(|_| finished) {
                    listener(start.get(), result.end_value);
                }
            });
        }
        // Read while measuring: each animation frame measures this node again.
        data.animatable.value()
    }
}

impl LayoutModifierNode for ContentSizeNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        let measured = placeable.size();
        let size = constraints.constrain(self.animate_to(measured));
        let offset = self.alignment.align(measured, size, scope.layout_direction());
        scope.place_at(&placeable, offset);
        scope.layout(size.width, size.height)
    }
}
