//! `Modifier.animate_content_size`, as Compose's `animateContentSize`: a node whose size follows
//! its content's, animated.

use compose_runtime::CoroutineScope;
use compose_ui::{
    Constraints, DetachContext, IntSize, LayoutModifierNode, Measurable, MeasureResult,
    MeasureScope, Modifier, ModifierNode, ModifierNodeElement, Size,
};

use crate::animatable::Animatable;
use crate::spec::{AnimationSpec, SpringSpec, spring};

/// The size-animating modifiers, on `Modifier`.
pub trait AnimationModifiers {
    /// Animates this node's size when its content's changes, along a spring, as
    /// `Modifier.animateContentSize()`. The content is laid out at its new size at once, placed
    /// at the top start and clipped to the animated size.
    fn animate_content_size(self) -> Modifier;
    /// The same along `animation_spec`.
    fn animate_content_size_with(self, animation_spec: impl Into<AnimationSpec>) -> Modifier;
}

impl AnimationModifiers for Modifier {
    fn animate_content_size(self) -> Modifier {
        // As Compose's default: medium-low stiffness, a pixel's threshold.
        self.animate_content_size_with(SpringSpec {
            stiffness: crate::Spring::STIFFNESS_MEDIUM_LOW,
            visibility_threshold: Some(1.0),
            ..spring()
        })
    }

    fn animate_content_size_with(self, animation_spec: impl Into<AnimationSpec>) -> Modifier {
        self.clip(compose_ui::RECTANGLE_SHAPE)
            .then_custom(ContentSizeElement { spec: animation_spec.into() })
    }
}

struct ContentSizeElement {
    spec: AnimationSpec,
}

struct ContentSizeNode {
    spec: AnimationSpec,
    /// The animated size, once measured.
    size: Option<Animatable<Size>>,
    /// The content's size the animation goes to.
    target: Option<IntSize>,
    scope: CoroutineScope,
}

impl ModifierNodeElement for ContentSizeElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(ContentSizeNode {
            spec: self.spec,
            size: None,
            target: None,
            scope: CoroutineScope::new(),
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<ContentSizeNode>() {
            node.spec = self.spec;
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<ContentSizeElement>().is_some_and(|other| other.spec == self.spec)
    }

    fn name(&self) -> &'static str {
        "animate_content_size"
    }
}

impl ModifierNode for ContentSizeNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }

    fn on_detach(&mut self, _context: &mut DetachContext<'_>) {
        self.scope.cancel();
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
        let size = self.size.get_or_insert_with(|| {
            Animatable::new(Size::new(measured.width as f32, measured.height as f32))
        });
        if self.target != Some(measured) {
            // The first measurement takes the size as it is; later ones animate to theirs.
            if self.target.is_some() {
                let (size, spec) = (size.clone(), self.spec);
                let target = Size::new(measured.width as f32, measured.height as f32);
                self.scope.launch(async move {
                    size.animate_to(target, spec).await;
                });
            }
            self.target = Some(measured);
        }
        // Read while measuring: each animation frame measures this node again.
        let current = size.value();
        let current = constraints
            .constrain(IntSize::new(current.width.round() as i32, current.height.round() as i32));
        scope.place(&placeable, 0, 0);
        scope.layout(current.width, current.height)
    }
}
