//! `Image`, as Compose's: a bitmap drawn in a node, scaled by a `ContentScale`.

use compose_runtime::composable;
use compose_ui::{
    Alignment, Constraints, DrawModifierNode, DrawScope, ImageBitmap, IntSize,
    IntrinsicMeasureScope, LayoutModifierNode, Measurable, MeasureResult, MeasureScope, Modifier,
    ModifierNode, ModifierNodeElement, Rect, Role, SemanticsConfiguration, SemanticsModifierNode,
    Size,
};

/// How an image's size maps to its node's: compose-ui's, as Compose's `ContentScale`.
pub use compose_ui::ContentScale;

/// A bitmap, as Compose's `Image(bitmap, contentDescription, …)`: its pixels' size within the
/// constraints, scaled by `content_scale` (a `FillWidth` banner as tall as its scaled width),
/// drawn scaled by `content_scale` and placed by `alignment` in its node, clipped to it. `content_description` is what accessibility reads;
/// `None` for decoration. `tint` draws its pixels all in that color, as `ColorFilter.tint`.
#[composable]
#[allow(non_snake_case)]
pub fn Image(
    bitmap: ImageBitmap,
    content_description: Option<impl Into<compose_ui::SharedString>>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Alignment::CENTER)] alignment: Alignment,
    #[default(crate::ContentScale::Fit)] content_scale: ContentScale,
    #[default(1.0)] alpha: f32,
    #[default(None)] tint: Option<compose_ui::Color>,
) {
    let element =
        ImageElement { bitmap, content_description, alignment, content_scale, alpha, tint };
    compose_ui::Layout(
        modifier.then_custom(element),
        compose_ui::empty_measure_policy(),
        compose_runtime::callback(|| {}),
    );
}

struct ImageElement {
    bitmap: ImageBitmap,
    content_description: Option<compose_ui::SharedString>,
    alignment: Alignment,
    content_scale: ContentScale,
    alpha: f32,
    tint: Option<compose_ui::Color>,
}

struct ImageNode {
    bitmap: ImageBitmap,
    content_description: Option<compose_ui::SharedString>,
    alignment: Alignment,
    content_scale: ContentScale,
    alpha: f32,
    tint: Option<compose_ui::Color>,
}

impl ModifierNodeElement for ImageElement {
    type Node = ImageNode;
    fn create(&self) -> ImageNode {
        ImageNode {
            bitmap: self.bitmap.clone(),
            content_description: self.content_description.clone(),
            alignment: self.alignment,
            content_scale: self.content_scale,
            alpha: self.alpha,
            tint: self.tint,
        }
    }

    fn update(&self, node: &mut ImageNode) {
        node.bitmap = self.bitmap.clone();
        node.content_description = self.content_description.clone();
        node.alignment = self.alignment;
        node.content_scale = self.content_scale;
        node.alpha = self.alpha;
        node.tint = self.tint;
    }

    fn same(&self, other: &Self) -> bool {
        other.bitmap.same_image(&self.bitmap)
            && other.content_description == self.content_description
            && other.alignment == self.alignment
            && other.content_scale == self.content_scale
            && other.alpha == self.alpha
            && other.tint == self.tint
    }

    fn name(&self) -> &'static str {
        "image"
    }
}

impl ModifierNode for ImageNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl ImageNode {
    /// Its pixels' size within `constraints`, scaled by its content scale, as Compose's
    /// painter node (`modifyConstraints`'s minimum): with fixed constraints, their size.
    fn sized(&self, constraints: Constraints) -> IntSize {
        let bounded = constraints.max_width != Constraints::INFINITY
            && constraints.max_height != Constraints::INFINITY;
        let fixed = constraints.min_width == constraints.max_width
            && constraints.min_height == constraints.max_height;
        if bounded && fixed {
            IntSize::new(constraints.max_width, constraints.max_height)
        } else {
            let (width, height) = (self.bitmap.width() as i32, self.bitmap.height() as i32);
            let constrained = Size::new(
                constraints.constrain_width(if width > 0 { width } else { constraints.min_width })
                    as f32,
                constraints.constrain_height(if height > 0 {
                    height
                } else {
                    constraints.min_height
                }) as f32,
            );
            let scaled = if constrained.width <= 0.0 || constrained.height <= 0.0 {
                Size::new(0.0, 0.0)
            } else {
                let source = Size::new(
                    if width > 0 { width as f32 } else { constrained.width },
                    if height > 0 { height as f32 } else { constrained.height },
                );
                let (x, y) = self.content_scale.scale_factor(source, constrained);
                Size::new(source.width * x, source.height * y)
            };
            IntSize::new(
                constraints.constrain_width(scaled.width.round() as i32),
                constraints.constrain_height(scaled.height.round() as i32),
            )
        }
    }
}

impl LayoutModifierNode for ImageNode {
    /// Its pixels' size within the constraints, scaled by its content scale ([`Self::sized`]).
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let size = self.sized(constraints);
        let placeable = scope.measure(content, Constraints::fixed(size.width, size.height));
        scope.place(&placeable, 0, 0);
        scope.layout(size.width, size.height)
    }

    // As Compose's painter node sized to its painter: at least its size at the other axis'
    // size, else the content's.
    fn min_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        let own = self.sized(Constraints::new(0, Constraints::INFINITY, 0, height)).width;
        own.max(scope.min_intrinsic_width(content, height))
    }

    fn max_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        let own = self.sized(Constraints::new(0, Constraints::INFINITY, 0, height)).width;
        own.max(scope.max_intrinsic_width(content, height))
    }

    fn min_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        let own = self.sized(Constraints::new(0, width, 0, Constraints::INFINITY)).height;
        own.max(scope.min_intrinsic_height(content, width))
    }

    fn max_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        let own = self.sized(Constraints::new(0, width, 0, Constraints::INFINITY)).height;
        own.max(scope.max_intrinsic_height(content, width))
    }
}

impl DrawModifierNode for ImageNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let source = Size::new(self.bitmap.width() as f32, self.bitmap.height() as f32);
        let bounds = scope.size();
        if source.width <= 0.0
            || source.height <= 0.0
            || bounds.width <= 0.0
            || bounds.height <= 0.0
        {
            return;
        }
        let (scale_x, scale_y) = self.content_scale.scale_factor(source, bounds);
        let drawn = IntSize::new(
            (source.width * scale_x).round() as i32,
            (source.height * scale_y).round() as i32,
        );
        let space = IntSize::new(bounds.width.round() as i32, bounds.height.round() as i32);
        let at = self.alignment.align(drawn, space, scope.layout_direction());
        // Clipped to the node, as Compose's `Image`.
        scope.save();
        scope.clip_outline(compose_ui::Outline::Rectangle(Rect::new(
            0.0,
            0.0,
            bounds.width,
            bounds.height,
        )));
        scope.draw_image_tinted(
            &self.bitmap,
            Rect::new(0.0, 0.0, source.width, source.height),
            Rect::new(
                at.x as f32,
                at.y as f32,
                (at.x + drawn.width) as f32,
                (at.y + drawn.height) as f32,
            ),
            self.alpha,
            self.tint,
        );
        scope.restore();
    }
}

impl SemanticsModifierNode for ImageNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        if let Some(description) = &self.content_description {
            config.set_content_description(description.clone());
            config.set_role(Role::Image);
        }
    }
}
