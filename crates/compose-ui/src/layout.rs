//! Measure and place: `MeasurePolicy`, `Measurable`, `Placeable`, `MeasureScope`, intrinsic
//! sizes (`IntrinsicMeasureScope`), and the `Layout` composable that `Box`, `Column` and `Row`
//! are written on (docs/06-ui.md, "Layout").
//!
//! One pass measures each node with constraints and places its children as it goes: a policy
//! calls `scope.measure` on its children, `scope.place` on the results, and returns its size with
//! `scope.layout`. A node measured again with the same constraints and nothing dirty inside
//! returns its size without running its policy.

use std::rc::Rc;

use compose_runtime::{composable, platform::current};

use crate::alignment::LayoutDirection;
use crate::constraints::Constraints;
use crate::modifier::{Modifier, ParentData};
use crate::text::TextMeasurer;
use crate::tree::{NodeId, UiTree};
use crate::unit::{Density, IntOffset, IntSize};

/// How a layout node measures its children and places them, as Compose's `MeasurePolicy`, and
/// what it answers when asked its intrinsic sizes.
///
/// The four intrinsic functions default to Compose's: they run `measure` in an intrinsic
/// measurement, where measuring a child returns a placeable sized by that child's intrinsic on
/// the axis asked (and by the constraint on the other one) and placing does nothing; the
/// result's width (or height) is the answer. A layout that knows its intrinsics more cheaply,
/// or whose `measure` has side effects, overrides them, as Compose's do.
pub trait MeasurePolicy: 'static {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult;

    /// The smallest width this layout can take at `height` and still paint its content
    /// correctly, as `minIntrinsicWidth`.
    fn min_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        Intrinsic::MinWidth.measured(scope, height, |scope, constraints| {
            self.measure(scope, measurables, constraints)
        })
    }

    /// The smallest height this layout can take at `width` and still paint its content
    /// correctly, as `minIntrinsicHeight`.
    fn min_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        Intrinsic::MinHeight.measured(scope, width, |scope, constraints| {
            self.measure(scope, measurables, constraints)
        })
    }

    /// The smallest width beyond which a larger one doesn't make it lower, at `height`, as
    /// `maxIntrinsicWidth`.
    fn max_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        Intrinsic::MaxWidth.measured(scope, height, |scope, constraints| {
            self.measure(scope, measurables, constraints)
        })
    }

    /// The smallest height beyond which a larger one doesn't make it narrower, at `width`, as
    /// `maxIntrinsicHeight`.
    fn max_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        Intrinsic::MaxHeight.measured(scope, width, |scope, constraints| {
            self.measure(scope, measurables, constraints)
        })
    }

    /// A name for dumps: the type's name.
    fn debug_name(&self) -> &'static str {
        crate::tree::short_type_name(std::any::type_name::<Self>())
    }
}

/// A closure measures as a policy, as a Kotlin lambda converts to the `MeasurePolicy` fun
/// interface: `Layout!(modifier, measure_policy(|scope, measurables, constraints| …), …)`. Its
/// intrinsics are the defaults (measured in an intrinsic measurement).
impl<F> MeasurePolicy for F
where
    F: Fn(&mut MeasureScope<'_>, &[Measurable], Constraints) -> MeasureResult + 'static,
{
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        self(scope, measurables, constraints)
    }

    fn debug_name(&self) -> &'static str {
        "Layout"
    }
}

/// `measure` as a policy for `Layout`, as Compose's `MeasurePolicy { measurables, constraints
/// -> … }`: the closure's argument types are inferred. A new closure is a new policy (the node
/// is measured again), as a Kotlin lambda capturing what changed; `remember` one that doesn't
/// change.
pub fn measure_policy(
    measure: impl Fn(&mut MeasureScope<'_>, &[Measurable], Constraints) -> MeasureResult + 'static,
) -> Rc<dyn MeasurePolicy> {
    Rc::new(measure)
}

/// Something to measure: a child of the node being measured, or, for a layout modifier, the
/// content it wraps (the rest of the chain and the node's own policy).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Measurable {
    pub(crate) node: NodeId,
    /// 0 for a child; for a modifier's content, the layer it measures.
    pub(crate) layer: u32,
}

/// What intrinsic sizes are asked of, as Compose's `IntrinsicMeasurable`: the same handle as
/// [`Measurable`], whose intrinsics an [`IntrinsicMeasureScope`] answers
/// (`scope.min_intrinsic_width(measurable, height)` for `measurable.minIntrinsicWidth(height)`).
pub type IntrinsicMeasurable = Measurable;

/// A measured `Measurable`, to place, as `Placeable`. Its size is within the constraints it was
/// measured with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Placeable {
    pub(crate) measurable: Measurable,
    pub width: i32,
    pub height: i32,
}

impl Placeable {
    pub fn size(&self) -> IntSize {
        IntSize::new(self.width, self.height)
    }
}

/// The size a policy or layout modifier measured, from [`MeasureScope::layout`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MeasureResult {
    pub width: i32,
    pub height: i32,
}

/// The largest size a layout may take on each axis, as Compose's: 2^24 - 1 pixels.
pub const MAX_LAYOUT_DIMENSION: i32 = (1 << 24) - 1;

/// Panics unless both sizes are within 0 and [`MAX_LAYOUT_DIMENSION`], as Compose's
/// `checkMeasuredSize`.
fn check_measured_size(width: i32, height: i32) {
    const MASK: i32 = !MAX_LAYOUT_DIMENSION;
    assert!(
        width & MASK == 0 && height & MASK == 0,
        "Size({width} x {height}) is out of range. Each dimension must be between 0 and \
         {MAX_LAYOUT_DIMENSION}."
    );
}

/// What asking the intrinsics of a layout that composes while it measures panics with, as
/// Compose's `SubcomposeLayout` (lazy lists and grids, `BoxWithConstraints`), word for word.
pub const NO_INTRINSICS_MESSAGE: &str = "Asking for intrinsic measurements of SubcomposeLayout \
    layouts is not supported. This includes components that are built on top of \
    SubcomposeLayout, such as lazy lists, BoxWithConstraints, TabRow, etc. To mitigate this:\n\
    - if intrinsic measurements are used to achieve 'match parent' sizing, consider replacing \
    the parent of the component with a custom layout which controls the order in which children \
    are measured, making intrinsic measurement not needed\n\
    - adding a size modifier to the component, in order to fast return the queried intrinsic \
    measurement.";

/// Which intrinsic an intrinsic measurement asks: what measuring a child returns in it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Intrinsic {
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
}

impl Intrinsic {
    /// The constraints a default intrinsic function measures with to answer for `value` (the
    /// size on the other axis), as Compose's: `Constraints(maxHeight = height)` for a width.
    pub(crate) fn constraints(self, value: i32) -> Constraints {
        match self {
            Intrinsic::MinWidth | Intrinsic::MaxWidth => {
                Constraints::new(0, Constraints::INFINITY, 0, value)
            }
            Intrinsic::MinHeight | Intrinsic::MaxHeight => {
                Constraints::new(0, value, 0, Constraints::INFINITY)
            }
        }
    }

    /// The size of `result` that answers: its width for a width.
    pub(crate) fn of(self, result: MeasureResult) -> i32 {
        match self {
            Intrinsic::MinWidth | Intrinsic::MaxWidth => result.width,
            Intrinsic::MinHeight | Intrinsic::MaxHeight => result.height,
        }
    }

    /// Compose's default intrinsic: `measure` run in an intrinsic measurement asking `self`.
    pub(crate) fn measured(
        self,
        scope: &mut IntrinsicMeasureScope<'_>,
        value: i32,
        measure: impl FnOnce(&mut MeasureScope<'_>, Constraints) -> MeasureResult,
    ) -> i32 {
        self.of(measure(&mut scope.intrinsics_scope(self), self.constraints(value)))
    }
}

/// The size an intrinsic measurement gives the axis it doesn't ask when that axis is
/// unbounded, as Compose's `LargeDimension` (2^15 - 1).
const LARGE_DIMENSION: i32 = (1 << 15) - 1;

/// What a policy or layout modifier asking intrinsic sizes works with, as Compose's
/// `IntrinsicMeasureScope`: the density, the layout direction, and its children's (or its
/// content's) intrinsics. A [`MeasureScope`] is one too (it dereferences to it), so a policy
/// asks its children's intrinsics while measuring.
pub struct IntrinsicMeasureScope<'a> {
    pub(crate) tree: &'a mut UiTree,
    /// The node whose policy or modifier asks.
    pub(crate) node: NodeId,
}

impl IntrinsicMeasureScope<'_> {
    pub fn density(&self) -> Density {
        self.tree.density
    }

    /// The direction of the layout, as `IntrinsicMeasureScope.layoutDirection`.
    pub fn layout_direction(&self) -> LayoutDirection {
        self.tree.node(self.node).layout_direction
    }

    /// compose-rs's optimizations beyond Compose's the host uses (`UiHost::set_optimizations`,
    /// D520): for measuring code with one (a text's `REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH`).
    pub fn optimizations(&self) -> crate::optimizations::Optimizations {
        self.tree.optimizations
    }

    /// The node being measured: what a lazy list premeasures its next items under
    /// (`PrefetchRequestScope::premeasure`).
    pub fn node_id(&self) -> NodeId {
        self.node
    }

    /// The window's (the root's) size: what popups and dialogs are measured within.
    pub fn window_size(&self) -> IntSize {
        self.tree.window
    }

    /// The window's insets (system bars, the keyboard, a cutout), as the platform reports them:
    /// a read observed as state is, so a change measures this node again.
    pub fn window_insets(&self) -> crate::configuration::PlatformWindowInsets {
        self.tree.window_insets.map(|state| state.get()).unwrap_or_default()
    }

    pub fn text_measurer(&mut self) -> &mut dyn TextMeasurer {
        &mut *self.tree.text_measurer
    }

    /// The data `measurable`'s modifiers give its parent (weight, alignment), as
    /// `IntrinsicMeasurable.parentData`.
    pub fn parent_data(&self, measurable: Measurable) -> ParentData {
        self.tree.node(measurable.node).parent_data
    }

    /// The smallest width `measurable` can take at `height` and still paint its content
    /// correctly, as `IntrinsicMeasurable.minIntrinsicWidth`.
    pub fn min_intrinsic_width(&mut self, measurable: IntrinsicMeasurable, height: i32) -> i32 {
        self.intrinsic(measurable, Intrinsic::MinWidth, height)
    }

    /// The smallest height `measurable` can take at `width` and still paint its content
    /// correctly, as `IntrinsicMeasurable.minIntrinsicHeight`.
    pub fn min_intrinsic_height(&mut self, measurable: IntrinsicMeasurable, width: i32) -> i32 {
        self.intrinsic(measurable, Intrinsic::MinHeight, width)
    }

    /// The smallest width beyond which a larger one doesn't make `measurable` lower, at
    /// `height`: the width it takes unconstrained, as `IntrinsicMeasurable.maxIntrinsicWidth`.
    pub fn max_intrinsic_width(&mut self, measurable: IntrinsicMeasurable, height: i32) -> i32 {
        self.intrinsic(measurable, Intrinsic::MaxWidth, height)
    }

    /// The smallest height beyond which a larger one doesn't make `measurable` narrower, at
    /// `width`, as `IntrinsicMeasurable.maxIntrinsicHeight`.
    pub fn max_intrinsic_height(&mut self, measurable: IntrinsicMeasurable, width: i32) -> i32 {
        self.intrinsic(measurable, Intrinsic::MaxHeight, width)
    }

    /// `measurable`'s intrinsic `kind` at `value` (the size on the other axis): a child's,
    /// which is then measured again, and whose change measures again the layout that asked
    /// (Compose's `intrinsicsUsageByParent`), or this node's content's (the rest of its chain).
    pub(crate) fn intrinsic(&mut self, measurable: Measurable, kind: Intrinsic, value: i32) -> i32 {
        if measurable.layer == 0 {
            assert_eq!(
                self.tree.parent_of(measurable.node),
                Some(self.node),
                "compose-ui: a layout asked the intrinsic size of a node that isn't its child"
            );
            let child = self.tree.node_mut(measurable.node);
            // Measured again after it was asked, as Compose's `onIntrinsicsQueried`: its
            // default intrinsics run its measuring code, which may keep what it saw.
            child.constraints = None;
            child.intrinsics_asked = true;
            self.tree.node_mut(self.node).children_intrinsics_asked = true;
        } else {
            assert_eq!(
                measurable.node, self.node,
                "compose-ui: a layout modifier asked the intrinsic size of another node's content"
            );
        }
        self.tree.intrinsic(measurable.node, measurable.layer as usize, kind, value)
    }

    /// A measure scope where measuring a child asks its intrinsic `kind` and placing does
    /// nothing, as Compose's `IntrinsicsMeasureScope`: what the default intrinsics measure in.
    pub(crate) fn intrinsics_scope(&mut self, kind: Intrinsic) -> MeasureScope<'_> {
        MeasureScope {
            base: IntrinsicMeasureScope { tree: &mut *self.tree, node: self.node },
            run: 0,
            relative: Vec::new(),
            intrinsic: Some(kind),
        }
    }
}

/// What a measure policy or layout modifier works with, as Compose's `MeasureScope` and
/// `Placeable.PlacementScope` in one. It dereferences to [`IntrinsicMeasureScope`].
pub struct MeasureScope<'a> {
    pub(crate) base: IntrinsicMeasureScope<'a>,
    /// This measurement, to reject a child measured twice in it.
    pub(crate) run: u64,
    /// What `place_relative` placed in a right-to-left layout, mirrored once the layout's
    /// width is known.
    pub(crate) relative: Vec<(Placeable, IntOffset)>,
    /// In an intrinsic measurement (a default intrinsic function measuring), the intrinsic
    /// asked: measuring returns it, placing does nothing.
    pub(crate) intrinsic: Option<Intrinsic>,
}

impl<'a> std::ops::Deref for MeasureScope<'a> {
    type Target = IntrinsicMeasureScope<'a>;

    fn deref(&self) -> &IntrinsicMeasureScope<'a> {
        &self.base
    }
}

impl std::ops::DerefMut for MeasureScope<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

impl<'a> MeasureScope<'a> {
    pub(crate) fn new(tree: &'a mut UiTree, node: NodeId, run: u64) -> MeasureScope<'a> {
        MeasureScope {
            base: IntrinsicMeasureScope { tree, node },
            run,
            relative: Vec::new(),
            intrinsic: None,
        }
    }

    /// Measures `measurable` with `constraints`.
    ///
    /// In an intrinsic measurement (a default intrinsic function running `measure`), returns a
    /// placeable sized by `measurable`'s intrinsic on the axis asked, and by the constraints'
    /// maximum on the other axis, as Compose's `DefaultIntrinsicMeasurable`.
    ///
    /// # Panics
    ///
    /// If this measurement already measured it, as Compose does: a second measurement would
    /// leave the layout depending on constraints no later pass replays.
    pub fn measure(&mut self, measurable: Measurable, constraints: Constraints) -> Placeable {
        if let Some(kind) = self.intrinsic {
            let other = |max: i32| if max == Constraints::INFINITY { LARGE_DIMENSION } else { max };
            let (width, height) = match kind {
                Intrinsic::MinWidth | Intrinsic::MaxWidth => (
                    self.base.intrinsic(measurable, kind, constraints.max_height),
                    other(constraints.max_height),
                ),
                Intrinsic::MinHeight | Intrinsic::MaxHeight => (
                    other(constraints.max_width),
                    self.base.intrinsic(measurable, kind, constraints.max_width),
                ),
            };
            return Placeable { measurable, width, height };
        }
        let node = self.base.node;
        let tree = &mut *self.base.tree;
        let size = if measurable.layer == 0 {
            assert_eq!(
                tree.parent_of(measurable.node),
                Some(node),
                "compose-ui: a policy measured a node that isn't its child"
            );
            let child = tree.node_mut(measurable.node);
            assert!(
                child.measured_in != self.run,
                "compose-ui: a measure policy measured the same child twice in one measurement"
            );
            child.measured_in = self.run;
            tree.measure_node(measurable.node, constraints)
        } else {
            assert_eq!(
                measurable.node, node,
                "compose-ui: a layout modifier measured another node's content"
            );
            let layer = &mut tree.node_mut(measurable.node).layers[measurable.layer as usize];
            assert!(
                layer.measured_in != self.run,
                "compose-ui: a layout modifier measured its content twice in one measurement"
            );
            layer.measured_in = self.run;
            tree.measure_layer(measurable.node, measurable.layer as usize, constraints)
        };
        Placeable { measurable, width: size.width, height: size.height }
    }

    /// Runs `action` after the layout, outside the tree, as the layout callbacks
    /// (`on_size_changed`) run: a text's `on_text_layout`. What it writes is laid out within the
    /// frame (D172). Nothing runs from an intrinsic measurement, which isn't the layout's.
    pub fn run_after_layout(&mut self, action: impl FnOnce() + 'static) {
        if self.intrinsic.is_none() {
            self.base.tree.layout_callbacks.push(crate::input::Action::Run(Box::new(action)));
        }
    }

    /// Whether this is an intrinsic measurement: a default intrinsic function running
    /// `measure` (Compose's `IntrinsicsMeasureScope`), where measuring returns intrinsics and
    /// placing does nothing. What it measures isn't the layout's.
    pub(crate) fn is_intrinsic_measurement(&self) -> bool {
        self.intrinsic.is_some()
    }

    /// Places `placeable` at `x`, `y` in this node's coordinates, as `Placeable.place`.
    pub fn place(&mut self, placeable: &Placeable, x: i32, y: i32) {
        self.place_at(placeable, IntOffset::new(x, y))
    }

    pub fn place_at(&mut self, placeable: &Placeable, position: IntOffset) {
        if self.intrinsic.is_some() {
            return;
        }
        let measurable = placeable.measurable;
        self.base.tree.place(measurable.node, measurable.layer as usize, position);
    }

    /// Places `placeable` at `x` from the start, as `placeRelative`: in a right-to-left
    /// layout, mirrored in the width the layout reports (`x` from its right edge).
    pub fn place_relative(&mut self, placeable: &Placeable, x: i32, y: i32) {
        if self.intrinsic.is_some() {
            return;
        }
        self.place(placeable, x, y);
        if self.layout_direction() == LayoutDirection::Rtl {
            self.relative.push((*placeable, IntOffset::new(x, y)));
        }
    }

    /// The result of a measurement of `width` by `height`, as `MeasureScope.layout`.
    ///
    /// # Panics
    ///
    /// If a size is negative or larger than [`MAX_LAYOUT_DIMENSION`], as Compose's (an
    /// intrinsic measurement takes a negative size as zero first, as Compose's).
    pub fn layout(&self, width: i32, height: i32) -> MeasureResult {
        let (width, height) =
            if self.intrinsic.is_some() { (width.max(0), height.max(0)) } else { (width, height) };
        check_measured_size(width, height);
        MeasureResult { width, height }
    }
}

/// A policy that measures every child with the constraints and stacks them at the origin,
/// taking the largest size: the root's policy, and Compose's default for a box.
pub struct StackMeasurePolicy;

impl MeasurePolicy for StackMeasurePolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let loose = constraints.loosen();
        let (mut width, mut height) = (constraints.min_width, constraints.min_height);
        for &measurable in measurables {
            let placeable = scope.measure(measurable, loose);
            scope.place_relative(&placeable, 0, 0);
            width = width.max(placeable.width);
            height = height.max(placeable.height);
        }
        scope.layout(width, height)
    }
}

/// A policy for a node without children: the smallest size the constraints allow.
pub struct EmptyMeasurePolicy;

impl MeasurePolicy for EmptyMeasurePolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        scope.layout(constraints.min_width, constraints.min_height)
    }
}

thread_local! {
    static EMPTY_POLICY: Rc<dyn MeasurePolicy> = Rc::new(EmptyMeasurePolicy);
}

/// The shared `EmptyMeasurePolicy`, so leaves share one allocation and compare `same`.
pub fn empty_measure_policy() -> Rc<dyn MeasurePolicy> {
    EMPTY_POLICY.with(Rc::clone)
}

/// The layout primitive, as Compose's `Layout`: a node with `modifier`, measured by
/// `measure_policy`, whose children are the nodes `content` emits.
#[composable]
pub fn Layout(
    #[default(Modifier)] modifier: Modifier,
    measure_policy: Rc<dyn MeasurePolicy>,
    content: impl Fn() + 'static,
) {
    emit_layout_node(modifier, measure_policy, || content());
}

/// Emits a layout node with `modifier` and `measure_policy`, and `content` as its children:
/// `Layout`'s body, for composables that build their node by hand. The node is reusable, as
/// Compose's `Layout` (`ReusableComposeNode`): content reused in its place keeps it.
pub fn emit_layout_node(
    mut modifier: Modifier,
    measure_policy: Rc<dyn MeasurePolicy>,
    content: impl FnOnce(),
) {
    // The clickables showing `LOCAL_INDICATION` show this node's (D1056).
    crate::clickable::resolve_local_indications(&mut modifier);
    let composer = current();
    let node = composer.start_reusable_node(compose_runtime::key_of("compose_ui::Layout"), || {
        crate::tree::LayoutNode::new(measure_policy.clone())
    });
    let direction = crate::alignment::LOCAL_LAYOUT_DIRECTION.current();
    composer.with_applier(|tree: &mut UiTree| {
        let id = tree.id_of(node);
        tree.set_layout_direction(id, direction);
        tree.set_measure_policy(id, measure_policy);
        tree.set_modifier(id, modifier);
    });
    content();
    composer.end_group();
}
