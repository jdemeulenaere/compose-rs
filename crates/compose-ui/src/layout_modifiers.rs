//! More layout modifiers, as Compose's: `aspectRatio`, `wrapContent*`, `defaultMinSize`,
//! `width`/`height`/`requiredWidth`/`requiredHeight(IntrinsicSize)` and
//! `Modifier.layout { measurable, constraints -> … }`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::alignment::{Alignment, HorizontalAlignment, VerticalAlignment};
use crate::constraints::Constraints;
use crate::layout::{Intrinsic, IntrinsicMeasureScope, Measurable, MeasureResult, MeasureScope};
use crate::modifier::{LayoutModifierNode, Modifier, ModifierNode, ModifierNodeElement};
use crate::node_context::{NodeContext, NodeHandle};
use crate::unit::{Density, Dp, IntOffset, IntSize};

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

    /// Its intrinsic `kind` at `value`, if it has its own; `None` for Compose's default
    /// (`measure` in an intrinsic measurement).
    fn intrinsic(
        &self,
        _scope: &mut IntrinsicMeasureScope<'_>,
        _content: Measurable,
        _kind: Intrinsic,
        _value: i32,
    ) -> Option<i32> {
        None
    }
}

/// A layout modifier that is its data: the element and its node in one.
#[derive(Clone, PartialEq)]
struct LayoutData<R>(R);

impl<R: LayoutRule> ModifierNodeElement for LayoutData<R> {
    type Node = Self;
    fn create(&self) -> Self {
        self.clone()
    }
    fn update(&self, node: &mut Self) {
        *node = self.clone();
    }
    fn same(&self, other: &Self) -> bool {
        other == self
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

impl<R: LayoutRule> LayoutData<R> {
    fn intrinsic(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        kind: Intrinsic,
        value: i32,
    ) -> i32 {
        match self.0.intrinsic(scope, content, kind, value) {
            Some(intrinsic) => intrinsic,
            None => kind.measured(scope, value, |scope, constraints| {
                self.0.measure(scope, content, constraints)
            }),
        }
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

    fn min_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        self.intrinsic(scope, content, Intrinsic::MinWidth, height)
    }

    fn min_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        self.intrinsic(scope, content, Intrinsic::MinHeight, width)
    }

    fn max_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        self.intrinsic(scope, content, Intrinsic::MaxWidth, height)
    }

    fn max_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        self.intrinsic(scope, content, Intrinsic::MaxHeight, width)
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

    /// As Compose's `AspectRatioNode`: from the other axis' size at the ratio when it's
    /// bounded, else the content's.
    fn intrinsic(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        kind: Intrinsic,
        value: i32,
    ) -> Option<i32> {
        if value == Constraints::INFINITY {
            return Some(scope.intrinsic(content, kind, value));
        }
        let round = crate::unit::round_to_int;
        Some(match kind {
            Intrinsic::MinWidth | Intrinsic::MaxWidth => round(value as f32 * self.ratio),
            Intrinsic::MinHeight | Intrinsic::MaxHeight => round(value as f32 / self.ratio),
        })
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

    /// As Compose's `UnspecifiedConstraintsNode`: the content's, at least the minimum.
    fn intrinsic(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        kind: Intrinsic,
        value: i32,
    ) -> Option<i32> {
        let minimum = match kind {
            Intrinsic::MinWidth | Intrinsic::MaxWidth => self.min_width,
            Intrinsic::MinHeight | Intrinsic::MaxHeight => self.min_height,
        };
        let minimum = if minimum.is_specified() { scope.density().round_to_px(minimum) } else { 0 };
        Some(scope.intrinsic(content, kind, value).max(minimum))
    }
}

/// An intrinsic size, as Compose's `IntrinsicSize`: what `Modifier::width`, `height`,
/// `required_width` and `required_height` size the content to when given one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IntrinsicSize {
    /// The content's minimum intrinsic size.
    Min,
    /// The content's maximum intrinsic size.
    Max,
}

/// What `Modifier::width`, `height`, `required_width` and `required_height` take, as their
/// Compose overloads: a size in dp (`width(40.dp())`) or an intrinsic size
/// (`width(IntrinsicSize::Max)`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SizeOrIntrinsic {
    Size(Dp),
    Intrinsic(IntrinsicSize),
}

impl From<Dp> for SizeOrIntrinsic {
    fn from(size: Dp) -> SizeOrIntrinsic {
        SizeOrIntrinsic::Size(size)
    }
}

impl From<IntrinsicSize> for SizeOrIntrinsic {
    fn from(size: IntrinsicSize) -> SizeOrIntrinsic {
        SizeOrIntrinsic::Intrinsic(size)
    }
}

impl Modifier {
    /// Sizes the content's width (`horizontal`) or height to its intrinsic `size`, as Compose's
    /// `IntrinsicWidthNode` and `IntrinsicHeightNode`: within the incoming constraints with
    /// `enforce_incoming` (`width(IntrinsicSize)`), else whatever they are
    /// (`requiredWidth(IntrinsicSize)`).
    pub(crate) fn intrinsic_size(
        self,
        horizontal: bool,
        size: IntrinsicSize,
        enforce_incoming: bool,
    ) -> Modifier {
        self.then_custom(LayoutData(IntrinsicSizeRule { horizontal, size, enforce_incoming }))
    }
}

#[derive(Clone, Copy, PartialEq)]
struct IntrinsicSizeRule {
    horizontal: bool,
    size: IntrinsicSize,
    enforce_incoming: bool,
}

impl IntrinsicSizeRule {
    /// The intrinsic of `size` on this rule's axis.
    fn own(&self) -> Intrinsic {
        match (self.horizontal, self.size) {
            (true, IntrinsicSize::Min) => Intrinsic::MinWidth,
            (true, IntrinsicSize::Max) => Intrinsic::MaxWidth,
            (false, IntrinsicSize::Min) => Intrinsic::MinHeight,
            (false, IntrinsicSize::Max) => Intrinsic::MaxHeight,
        }
    }
}

impl LayoutRule for IntrinsicSizeRule {
    const NAME: &'static str = "intrinsic_size";

    /// As Compose's `IntrinsicSizeModifier.measure`: the content at its intrinsic size on the
    /// axis (unless enforcing incoming constraints that fix it), within the incoming
    /// constraints when enforcing them.
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let fixed = if self.horizontal {
            constraints.has_fixed_width()
        } else {
            constraints.has_fixed_height()
        };
        let content_constraints = if self.enforce_incoming && fixed {
            constraints
        } else if self.horizontal {
            let width = scope.intrinsic(content, self.own(), constraints.max_height).max(0);
            Constraints::new(width, width, 0, Constraints::INFINITY)
        } else {
            let height = scope.intrinsic(content, self.own(), constraints.max_width).max(0);
            Constraints::new(0, Constraints::INFINITY, height, height)
        };
        let placeable = scope.measure(
            content,
            if self.enforce_incoming {
                constraints.constrain_by(content_constraints)
            } else {
                content_constraints
            },
        );
        scope.place_relative(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }

    /// On its axis, both intrinsics are the content's of its size; across it, the content's.
    fn intrinsic(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        kind: Intrinsic,
        value: i32,
    ) -> Option<i32> {
        let on_axis = match kind {
            Intrinsic::MinWidth | Intrinsic::MaxWidth => self.horizontal,
            Intrinsic::MinHeight | Intrinsic::MaxHeight => !self.horizontal,
        };
        Some(scope.intrinsic(content, if on_axis { self.own() } else { kind }, value))
    }
}

type MeasureFunction = Rc<dyn Fn(&mut MeasureScope<'_>, Measurable, Constraints) -> MeasureResult>;

/// `Modifier.layout`'s closure, compared by identity.
#[derive(Clone)]
struct LayoutElement(MeasureFunction);

impl ModifierNodeElement for LayoutElement {
    type Node = Self;
    fn create(&self) -> Self {
        self.clone()
    }
    fn update(&self, node: &mut Self) {
        *node = self.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&other.0, &self.0)
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

/// What an offset moves by: fixed, or read while placing.
#[derive(Clone)]
enum OffsetBy {
    Dp(Dp, Dp),
    Px(Rc<dyn Fn(Density) -> IntOffset>),
}

/// `Modifier.offset` and its kinds, as Compose's `OffsetNode` and `OffsetPxNode`: placed in a block,
/// so a new offset, or what its block reads, places it again and measures nothing.
pub(crate) struct OffsetElement {
    by: OffsetBy,
    rtl_aware: bool,
}

impl OffsetElement {
    pub(crate) fn dp(x: Dp, y: Dp, rtl_aware: bool) -> OffsetElement {
        OffsetElement { by: OffsetBy::Dp(x, y), rtl_aware }
    }

    pub(crate) fn px(by: Rc<dyn Fn(Density) -> IntOffset>, rtl_aware: bool) -> OffsetElement {
        OffsetElement { by: OffsetBy::Px(by), rtl_aware }
    }
}

pub(crate) struct OffsetNode {
    /// What its block reads, its update changing it.
    by: Rc<RefCell<(OffsetBy, bool)>>,
    handle: Option<NodeHandle>,
}

impl ModifierNodeElement for OffsetElement {
    type Node = OffsetNode;

    fn create(&self) -> OffsetNode {
        OffsetNode { by: Rc::new(RefCell::new((self.by.clone(), self.rtl_aware))), handle: None }
    }

    fn update(&self, node: &mut OffsetNode) {
        *node.by.borrow_mut() = (self.by.clone(), self.rtl_aware);
        if let Some(handle) = node.handle {
            handle.invalidate_placement();
        }
    }

    fn same(&self, other: &Self) -> bool {
        self.rtl_aware == other.rtl_aware
            && match (&self.by, &other.by) {
                (OffsetBy::Dp(a, b), OffsetBy::Dp(c, d)) => a == c && b == d,
                (OffsetBy::Px(a), OffsetBy::Px(b)) => Rc::ptr_eq(a, b),
                _ => false,
            }
    }

    fn name(&self) -> &'static str {
        "offset"
    }
}

impl ModifierNode for OffsetNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }

    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.handle = Some(context.handle());
    }

    /// Its update asks for what it needs: a placing.
    fn should_auto_invalidate(&self) -> bool {
        false
    }
}

impl LayoutModifierNode for OffsetNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        let by = self.by.clone();
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            let (offset, rtl_aware) = by.borrow().clone();
            let density = placement.density();
            let (x, y) = match offset {
                OffsetBy::Dp(x, y) => (density.round_to_px(x), density.round_to_px(y)),
                OffsetBy::Px(offset) => {
                    let at = offset(density);
                    (at.x, at.y)
                }
            };
            if rtl_aware {
                placement.place_relative(&placeable, x, y);
            } else {
                placement.place(&placeable, x, y);
            }
        })
    }
}
