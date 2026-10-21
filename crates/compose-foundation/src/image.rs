//! `Image`, as Compose's: a bitmap drawn in a node, scaled by a `ContentScale`.

use compose_runtime::composable;
use compose_ui::{
    Alignment, Constraints, DrawModifierNode, DrawScope, ImageBitmap, IntSize, LayoutModifierNode,
    Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement, Rect,
    Role, SemanticsConfiguration, SemanticsModifierNode, Size,
};

/// How an image's size maps to its node's, as Compose's `ContentScale`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContentScale {
    /// Scaled to fit inside, its aspect ratio kept.
    Fit,
    /// Scaled to cover, its aspect ratio kept (the rest clipped).
    Crop,
    /// Stretched to the node's size.
    FillBounds,
    /// Scaled to the node's width, its aspect ratio kept.
    FillWidth,
    /// Scaled to the node's height, its aspect ratio kept.
    FillHeight,
    /// Scaled down to fit inside if larger, never up.
    Inside,
    /// At its own size.
    None,
}

impl compose_runtime::ComposeArg for ContentScale {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ContentScale {
    /// The scale factors from a `source` size to a `destination` size, as `computeScaleFactor`.
    pub fn scale_factor(self, source: Size, destination: Size) -> (f32, f32) {
        let (width, height) =
            (destination.width / source.width, destination.height / source.height);
        let uniform = |scale: f32| (scale, scale);
        match self {
            ContentScale::Fit => uniform(width.min(height)),
            ContentScale::Crop => uniform(width.max(height)),
            ContentScale::FillBounds => (width, height),
            ContentScale::FillWidth => uniform(width),
            ContentScale::FillHeight => uniform(height),
            ContentScale::Inside => uniform(width.min(height).min(1.0)),
            ContentScale::None => uniform(1.0),
        }
    }
}

/// A bitmap, as Compose's `Image(bitmap, contentDescription, …)`: as large as its pixels,
/// within the constraints (its aspect ratio kept), drawn scaled by `content_scale` and placed by
/// `alignment` in its node, clipped to it. `content_description` is what accessibility reads;
/// `None` for decoration.
#[composable]
#[allow(non_snake_case)]
pub fn Image(
    bitmap: ImageBitmap,
    content_description: Option<&'static str>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Alignment::CENTER)] alignment: Alignment,
    #[default(crate::ContentScale::Fit)] content_scale: ContentScale,
    #[default(1.0)] alpha: f32,
) {
    let element = ImageElement { bitmap, content_description, alignment, content_scale, alpha };
    compose_ui::Layout(
        modifier.then_custom(element),
        compose_ui::empty_measure_policy(),
        compose_runtime::callback(|| {}),
    );
}

struct ImageElement {
    bitmap: ImageBitmap,
    content_description: Option<&'static str>,
    alignment: Alignment,
    content_scale: ContentScale,
    alpha: f32,
}

struct ImageNode {
    bitmap: ImageBitmap,
    content_description: Option<&'static str>,
    alignment: Alignment,
    content_scale: ContentScale,
    alpha: f32,
}

impl ModifierNodeElement for ImageElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(ImageNode {
            bitmap: self.bitmap.clone(),
            content_description: self.content_description,
            alignment: self.alignment,
            content_scale: self.content_scale,
            alpha: self.alpha,
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<ImageNode>() {
            node.bitmap = self.bitmap.clone();
            node.content_description = self.content_description;
            node.alignment = self.alignment;
            node.content_scale = self.content_scale;
            node.alpha = self.alpha;
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<ImageElement>().is_some_and(|other| {
            other.bitmap.same_image(&self.bitmap)
                && other.content_description == self.content_description
                && other.alignment == self.alignment
                && other.content_scale == self.content_scale
                && other.alpha == self.alpha
        })
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

impl LayoutModifierNode for ImageNode {
    /// Its pixels' size within the constraints, scaled down (its aspect ratio kept) to fit, as
    /// Compose's painter node.
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let intrinsic = Size::new(self.bitmap.width() as f32, self.bitmap.height() as f32);
        let size = if constraints.min_width == constraints.max_width
            && constraints.min_height == constraints.max_height
        {
            IntSize::new(constraints.max_width, constraints.max_height)
        } else if intrinsic.width <= 0.0 || intrinsic.height <= 0.0 {
            IntSize::new(constraints.min_width, constraints.min_height)
        } else {
            let bound =
                |max: i32| if max == Constraints::INFINITY { f32::INFINITY } else { max as f32 };
            let scale = (bound(constraints.max_width) / intrinsic.width)
                .min(bound(constraints.max_height) / intrinsic.height)
                .min(1.0);
            constraints.constrain(IntSize::new(
                (intrinsic.width * scale).round() as i32,
                (intrinsic.height * scale).round() as i32,
            ))
        };
        let placeable = scope.measure(content, Constraints::fixed(size.width, size.height));
        scope.place(&placeable, 0, 0);
        scope.layout(size.width, size.height)
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
        let at = self.alignment.align(drawn, space, compose_ui::LayoutDirection::Ltr);
        // Clipped to the node, as Compose's `Image`.
        scope.save();
        scope.clip_outline(compose_ui::Outline::Rectangle(Rect::new(
            0.0,
            0.0,
            bounds.width,
            bounds.height,
        )));
        scope.draw_image(
            &self.bitmap,
            Rect::new(0.0, 0.0, source.width, source.height),
            Rect::new(
                at.x as f32,
                at.y as f32,
                (at.x + drawn.width) as f32,
                (at.y + drawn.height) as f32,
            ),
            self.alpha,
        );
        scope.restore();
    }
}

impl SemanticsModifierNode for ImageNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        if let Some(description) = self.content_description {
            config.content_description.push(description.into());
            config.role = Some(Role::Image);
        }
    }
}
