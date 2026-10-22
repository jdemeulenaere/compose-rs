//! More layout modifiers, as Compose's: `aspectRatio`, `wrapContent*`, `defaultMinSize` and
//! `Modifier.layout { measurable, constraints -> … }`.

use std::rc::Rc;

use crate::alignment::{Alignment, HorizontalAlignment, VerticalAlignment};
use crate::constraints::Constraints;
use crate::layout::{Measurable, MeasureResult, MeasureScope};
use crate::modifier::{LayoutModifierNode, Modifier, ModifierNode, ModifierNodeElement};
use crate::unit::{Dp, IntSize};

impl Modifier {
    /// Sizes the content to `ratio` (width / height), as `Modifier.aspectRatio`: the largest
    /// size the incoming constraints allow, trying the maximum width first (the maximum height
    /// with `match_height_constraints_first`), then the minimums, then the same without keeping
    /// to the constraints.
    ///
    /// # Panics
    ///
    /// If `ratio` isn't positive.
    pub fn aspect_ratio(self, ratio: f32) -> Modifier {
        self.aspect_ratio_with(ratio, false)
    }

    /// `aspect_ratio` with `match_height_constraints_first`, as
    /// `Modifier.aspectRatio(ratio, matchHeightConstraintsFirst)`.
    pub fn aspect_ratio_with(self, ratio: f32, match_height_constraints_first: bool) -> Modifier {
        assert!(ratio > 0.0, "compose-ui: aspect_ratio must be positive, was {ratio}");
        self.then_custom(LayoutData(AspectRatio { ratio, match_height_constraints_first }))
    }

    /// Lets the content take its own width, aligned by `align` in the width the constraints
    /// impose, as `Modifier.wrapContentWidth(align, unbounded)`; `unbounded`, it may be wider
    /// than the maximum.
    pub fn wrap_content_width(self) -> Modifier {
        self.wrap_content_width_with(Alignment::CENTER_HORIZONTALLY, false)
    }

    /// `wrap_content_width` with `align` and `unbounded`, as `Modifier.wrapContentWidth(align,
    /// unbounded)`.
    pub fn wrap_content_width_with(self, align: HorizontalAlignment, unbounded: bool) -> Modifier {
        let wrap = WrapContent {
            horizontal: true,
            vertical: false,
            align: Alignment::new(align.bias, -1.0),
            unbounded,
        };
        self.then_custom(LayoutData(wrap))
    }

    /// As `wrap_content_width`, for the height, as `Modifier.wrapContentHeight`.
    pub fn wrap_content_height(self) -> Modifier {
        self.wrap_content_height_with(Alignment::CENTER_VERTICALLY, false)
    }

    /// `wrap_content_height` with `align` and `unbounded`.
    pub fn wrap_content_height_with(self, align: VerticalAlignment, unbounded: bool) -> Modifier {
        let wrap = WrapContent {
            horizontal: false,
            vertical: true,
            align: Alignment::new(-1.0, align.bias),
            unbounded,
        };
        self.then_custom(LayoutData(wrap))
    }

    /// As `wrap_content_width`, for both directions, as `Modifier.wrapContentSize`.
    pub fn wrap_content_size(self) -> Modifier {
        self.wrap_content_size_with(Alignment::CENTER, false)
    }

    /// `wrap_content_size` with `align` and `unbounded`.
    pub fn wrap_content_size_with(self, align: Alignment, unbounded: bool) -> Modifier {
        self.then_custom(LayoutData(WrapContent {
            horizontal: true,
            vertical: true,
            align,
            unbounded,
        }))
    }

    /// A minimum size only where the incoming constraints set none (a zero minimum), as
    /// `Modifier.defaultMinSize` (`Dp::UNSPECIFIED`: none).
    pub fn default_min_size(self, min_width: Dp, min_height: Dp) -> Modifier {
        self.then_custom(LayoutData(DefaultMinSize { min_width, min_height }))
    }

    /// Measures and places the content with `measure`, as `Modifier.layout { measurable,
    /// constraints -> … }`: it measures the content (the rest of the chain and the node) and
    /// places it, returning this layer's size. State it reads measures it again. A new
    /// closure is a new modifier (the node is measured again), as Compose's lambda.
    pub fn layout(
        self,
        measure: impl Fn(&mut MeasureScope<'_>, Measurable, Constraints) -> MeasureResult + 'static,
    ) -> Modifier {
        self.then_custom(LayoutElement(Rc::new(measure)))
    }
}

/// What a data-only layout modifier measures with.
trait LayoutRule: PartialEq + Clone + 'static {
    const NAME: &'static str;
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult;
}

/// A layout modifier that is its data: the element and its node in one.
#[derive(Clone, PartialEq)]
struct LayoutData<R>(R);

impl<R: LayoutRule> ModifierNodeElement for LayoutData<R> {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(self.clone())
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<LayoutData<R>>() {
            *node = self.clone();
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<LayoutData<R>>().is_some_and(|other| other == self)
    }
    fn name(&self) -> &'static str {
        R::NAME
    }
}

impl<R: LayoutRule> ModifierNode for LayoutData<R> {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl<R: LayoutRule> LayoutModifierNode for LayoutData<R> {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        self.0.measure(scope, content, constraints)
    }
}

#[derive(Clone, Copy, PartialEq)]
struct AspectRatio {
    ratio: f32,
    match_height_constraints_first: bool,
}

impl AspectRatio {
    /// As Compose's `AspectRatioNode.findSize`: `None` if no size fits.
    fn find_size(&self, constraints: Constraints) -> Option<IntSize> {
        // (from the width, from the maximum): `tryMaxWidth`, `tryMaxHeight`, `tryMinWidth`,
        // `tryMinHeight`.
        let order = if self.match_height_constraints_first {
            [(false, true), (true, true), (false, false), (true, false)]
        } else {
            [(true, true), (false, true), (true, false), (false, false)]
        };
        [true, false].into_iter().find_map(|enforce| {
            order
                .into_iter()
                .find_map(|(width, max)| self.try_size(constraints, width, max, enforce))
        })
    }

    /// The size at the ratio from the constraints' maximum or minimum width (or height), if
    /// positive and, with `enforce`, within the constraints.
    fn try_size(
        &self,
        constraints: Constraints,
        from_width: bool,
        from_max: bool,
        enforce: bool,
    ) -> Option<IntSize> {
        let round = crate::unit::round_to_int;
        let size = match (from_width, from_max) {
            (true, true) => constraints.max_width,
            (true, false) => constraints.min_width,
            (false, true) => constraints.max_height,
            (false, false) => constraints.min_height,
        };
        if size == Constraints::INFINITY {
            return None;
        }
        let size = if from_width {
            IntSize::new(size, round(size as f32 / self.ratio))
        } else {
            IntSize::new(round(size as f32 * self.ratio), size)
        };
        let satisfies = (constraints.min_width..=constraints.max_width).contains(&size.width)
            && (constraints.min_height..=constraints.max_height).contains(&size.height);
        (size.width > 0 && size.height > 0 && (!enforce || satisfies)).then_some(size)
    }
}

impl LayoutRule for AspectRatio {
    const NAME: &'static str = "aspect_ratio";

    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let wrapped = match self.find_size(constraints) {
            Some(size) => Constraints::fixed(size.width, size.height),
            None => constraints,
        };
        let placeable = scope.measure(content, wrapped);
        scope.place_relative(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

#[derive(Clone, Copy, PartialEq)]
struct WrapContent {
    horizontal: bool,
    vertical: bool,
    align: Alignment,
    unbounded: bool,
}

impl LayoutRule for WrapContent {
    const NAME: &'static str = "wrap_content";

    /// As Compose's `WrapContentNode`.
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let wrapped = Constraints::new(
            if self.horizontal { 0 } else { constraints.min_width },
            if self.horizontal && self.unbounded {
                Constraints::INFINITY
            } else {
                constraints.max_width
            },
            if self.vertical { 0 } else { constraints.min_height },
            if self.vertical && self.unbounded {
                Constraints::INFINITY
            } else {
                constraints.max_height
            },
        );
        let placeable = scope.measure(content, wrapped);
        let width = placeable.width.clamp(constraints.min_width, constraints.max_width);
        let height = placeable.height.clamp(constraints.min_height, constraints.max_height);
        let position = self.align.align(
            placeable.size(),
            IntSize::new(width, height),
            scope.layout_direction(),
        );
        scope.place(&placeable, position.x, position.y);
        scope.layout(width, height)
    }
}

#[derive(Clone, Copy, PartialEq)]
struct DefaultMinSize {
    min_width: Dp,
    min_height: Dp,
}

impl LayoutRule for DefaultMinSize {
    const NAME: &'static str = "default_min_size";

    /// As Compose's `UnspecifiedConstraintsNode`.
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let min = |dp: Dp, min: i32, max: i32| {
            if dp.is_specified() && min == 0 {
                density.round_to_px(dp).min(max).max(0)
            } else {
                min
            }
        };
        let wrapped = Constraints::new(
            min(self.min_width, constraints.min_width, constraints.max_width),
            constraints.max_width,
            min(self.min_height, constraints.min_height, constraints.max_height),
            constraints.max_height,
        );
        let placeable = scope.measure(content, wrapped);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

type MeasureFunction = Rc<dyn Fn(&mut MeasureScope<'_>, Measurable, Constraints) -> MeasureResult>;

/// `Modifier.layout`'s closure, compared by identity.
#[derive(Clone)]
struct LayoutElement(MeasureFunction);

impl ModifierNodeElement for LayoutElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(self.clone())
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<LayoutElement>() {
            *node = self.clone();
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<LayoutElement>().is_some_and(|other| Rc::ptr_eq(&other.0, &self.0))
    }
    fn name(&self) -> &'static str {
        "layout"
    }
}

impl ModifierNode for LayoutElement {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for LayoutElement {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        (self.0)(scope, content, constraints)
    }
}
