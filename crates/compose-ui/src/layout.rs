//! Measure and place: `MeasurePolicy`, `Measurable`, `Placeable`, `MeasureScope`, and the
//! `Layout` composable that `Box`, `Column` and `Row` are written on (docs/06-ui.md, "Layout").
//!
//! One pass measures each node with constraints and places its children as it goes: a policy
//! calls `scope.measure` on its children, `scope.place` on the results, and returns its size with
//! `scope.layout`. A node measured again with the same constraints and nothing dirty inside
//! returns its size without running its policy.

use std::rc::Rc;

use compose_runtime::{composable, current};

use crate::alignment::LayoutDirection;
use crate::constraints::Constraints;
use crate::modifier::{Modifier, ParentData};
use crate::text::TextMeasurer;
use crate::tree::{NodeId, UiTree};
use crate::unit::{Density, IntOffset, IntSize};

/// How a layout node measures its children and places them, as Compose's `MeasurePolicy`.
pub trait MeasurePolicy: 'static {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult;

    /// A name for dumps: the type's name.
    fn debug_name(&self) -> &'static str {
        crate::tree::short_type_name(std::any::type_name::<Self>())
    }
}

/// Something to measure: a child of the node being measured, or, for a layout modifier, the
/// content it wraps (the rest of the chain and the node's own policy).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Measurable {
    pub(crate) node: NodeId,
    /// 0 for a child; for a modifier's content, the layer it measures.
    pub(crate) layer: u32,
}

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

/// What a measure policy or layout modifier works with, as Compose's `MeasureScope` and
/// `Placeable.PlacementScope` in one.
pub struct MeasureScope<'a> {
    pub(crate) tree: &'a mut UiTree,
    pub(crate) node: NodeId,
    /// This measurement, to reject a child measured twice in it.
    pub(crate) run: u64,
}

impl MeasureScope<'_> {
    pub fn density(&self) -> Density {
        self.tree.density
    }

    /// The window's (the root's) size: what popups and dialogs are measured within.
    pub fn window_size(&self) -> IntSize {
        self.tree.window
    }

    pub fn layout_direction(&self) -> LayoutDirection {
        LayoutDirection::Ltr
    }

    pub fn text_measurer(&mut self) -> &mut dyn TextMeasurer {
        &mut *self.tree.text_measurer
    }

    /// Measures `measurable` with `constraints`.
    ///
    /// # Panics
    ///
    /// If this measurement already measured it, as Compose does: a second measurement would
    /// leave the layout depending on constraints no later pass replays.
    pub fn measure(&mut self, measurable: Measurable, constraints: Constraints) -> Placeable {
        let size = if measurable.layer == 0 {
            assert_eq!(
                self.tree.parent_of(measurable.node),
                Some(self.node),
                "compose-ui: a policy measured a node that isn't its child"
            );
            let child = self.tree.node_mut(measurable.node);
            assert!(
                child.measured_in != self.run,
                "compose-ui: a measure policy measured the same child twice in one measurement"
            );
            child.measured_in = self.run;
            self.tree.measure_node(measurable.node, constraints)
        } else {
            assert_eq!(
                measurable.node, self.node,
                "compose-ui: a layout modifier measured another node's content"
            );
            let layer = &mut self.tree.node_mut(measurable.node).layers[measurable.layer as usize];
            assert!(
                layer.measured_in != self.run,
                "compose-ui: a layout modifier measured its content twice in one measurement"
            );
            layer.measured_in = self.run;
            self.tree.measure_layer(measurable.node, measurable.layer as usize, constraints)
        };
        Placeable { measurable, width: size.width, height: size.height }
    }

    /// Places `placeable` at `x`, `y` in this node's coordinates, as `Placeable.place`.
    pub fn place(&mut self, placeable: &Placeable, x: i32, y: i32) {
        self.place_at(placeable, IntOffset::new(x, y))
    }

    pub fn place_at(&mut self, placeable: &Placeable, position: IntOffset) {
        let measurable = placeable.measurable;
        self.tree.place(measurable.node, measurable.layer as usize, position);
    }

    /// Places in the layout direction: as `place`, until right-to-left layout exists.
    pub fn place_relative(&mut self, placeable: &Placeable, x: i32, y: i32) {
        self.place(placeable, x, y)
    }

    /// The result of a measurement of `width` by `height`, as `MeasureScope.layout`.
    pub fn layout(&self, width: i32, height: i32) -> MeasureResult {
        MeasureResult { width, height }
    }

    /// The data `measurable`'s modifiers give its parent (weight, alignment).
    pub fn parent_data(&self, measurable: Measurable) -> ParentData {
        self.tree.node(measurable.node).parent_data
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
            scope.place(&placeable, 0, 0);
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
/// `Layout`'s body, for composables that build their node by hand.
pub fn emit_layout_node(
    modifier: Modifier,
    measure_policy: Rc<dyn MeasurePolicy>,
    content: impl FnOnce(),
) {
    let composer = current();
    let node = composer.start_node(compose_runtime::key_of("compose_ui::Layout"), || {
        crate::tree::LayoutNode::new(measure_policy.clone())
    });
    composer.with_applier(|tree: &mut UiTree| {
        let id = tree.id_of(node);
        tree.set_measure_policy(id, measure_policy);
        tree.set_modifier(id, modifier);
    });
    content();
    composer.end_group();
}
