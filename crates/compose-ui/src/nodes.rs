//! The nodes of the built-in modifier elements. Each keeps its element's data, updated in place
//! on recomposition, and its own state (pressed, focused).

use std::rc::Rc;

use compose_runtime::Callback;

use crate::constraints::Constraints;
use crate::draw::DrawScope;
use crate::graphics::{Color, Shape};
use crate::input::{EventContext, KeyEvent, PointerEvent, PointerEventPass, PointerEventType};
use crate::layout::{Intrinsic, IntrinsicMeasureScope, Measurable, MeasureResult, MeasureScope};
use crate::modifier::{
    DrawModifierNode, Element, Fill, FocusTargetModifierNode, KeyInputModifierNode,
    LayoutModifierNode, ModifierNode, ModifierNodeElement, PaddingValues, PointerInputModifierNode,
    SemanticsModifierNode, SizeSpec,
};
use crate::semantics::SemanticsConfiguration;
use crate::text::SharedString;
use crate::unit::{Dp, IntSize};

/// The node of an element.
pub(crate) fn create(element: &Element) -> Box<dyn ModifierNode> {
    match element {
        Element::Padding(padding) => Box::new(PaddingNode(*padding)),
        Element::Offset(x, y) => Box::new(OffsetNode(*x, *y)),
        Element::MinimumTouchTarget(size) => Box::new(MinimumTouchTargetNode(*size)),
        Element::Size(spec) => Box::new(SizeNode(*spec)),
        Element::Fill(fill) => Box::new(FillNode(*fill)),
        // Parent data is read from the elements; these nodes do nothing. A zero-sized box
        // doesn't allocate.
        Element::Weight(..)
        | Element::Align(_)
        | Element::FillCrossAxis(_)
        | Element::ZIndex(_) => Box::new(Inert),
        Element::LazyItemBoundary => Box::new(crate::graphics_layer::RepaintBoundaryNode),
        Element::Background(color, shape) => Box::new(BackgroundNode(*color, shape.clone())),
        Element::Border(width, color, shape) => Box::new(crate::border::BorderNode {
            width: *width,
            brush: crate::brush::Brush::Solid(*color),
            shape: shape.clone(),
        }),
        // A graphics layer clipping to its shape, as Compose's `clip` (D523).
        Element::Clip(shape) => {
            Box::new(crate::graphics_layer::GraphicsLayerNode::clip(shape.clone()))
        }
        Element::Clickable(clickable) => {
            Box::new(crate::clickable::ClickableNode::new(clickable.clone(), None))
        }
        Element::Focusable(source) => Box::new(FocusableNode {
            interaction_source: source.clone(),
            focused: false,
            focus: None,
            emitter: Default::default(),
            handle: None,
            scope: None,
        }),
        Element::OnFocusChanged(callback) => Box::new(OnFocusChangedNode {
            callback: callback.clone(),
            last: Rc::new(std::cell::Cell::new(None)),
        }),
        Element::OnKeyEvent(callback) => Box::new(OnKeyEventNode(callback.clone())),
        Element::TestTag(tag) => Box::new(TestTagNode(tag.clone())),
        Element::Semantics(merge, properties) => {
            Box::new(SemanticsNode(*merge, properties.clone()))
        }
        Element::Custom(element) => element.create_boxed(),
    }
}

/// Updates `node`, which an element of the same kind created, to `element`.
pub(crate) fn update(element: &Element, node: &mut dyn ModifierNode) {
    macro_rules! set {
        ($type:ty, $value:expr) => {
            if let Some(node) = node.downcast_mut::<$type>() {
                *node = $value;
            }
        };
    }
    match element {
        Element::Padding(padding) => set!(PaddingNode, PaddingNode(*padding)),
        Element::Offset(x, y) => set!(OffsetNode, OffsetNode(*x, *y)),
        Element::MinimumTouchTarget(size) => {
            set!(MinimumTouchTargetNode, MinimumTouchTargetNode(*size))
        }
        Element::Size(spec) => set!(SizeNode, SizeNode(*spec)),
        Element::Fill(fill) => set!(FillNode, FillNode(*fill)),
        Element::Weight(..)
        | Element::Align(_)
        | Element::FillCrossAxis(_)
        | Element::ZIndex(_)
        | Element::LazyItemBoundary => {}
        Element::Background(color, shape) => {
            set!(BackgroundNode, BackgroundNode(*color, shape.clone()))
        }
        Element::Border(width, color, shape) => set!(
            crate::border::BorderNode,
            crate::border::BorderNode {
                width: *width,
                brush: crate::brush::Brush::Solid(*color),
                shape: shape.clone()
            }
        ),
        Element::Clip(shape) => set!(
            crate::graphics_layer::GraphicsLayerNode,
            crate::graphics_layer::GraphicsLayerNode::clip(shape.clone())
        ),
        Element::Clickable(clickable) => {
            if let Some(node) = node.downcast_mut::<crate::clickable::ClickableNode>() {
                node.update(clickable.clone(), None);
            }
        }
        Element::Focusable(source) => {
            if let Some(node) = node.downcast_mut::<FocusableNode>()
                && node.interaction_source != *source
            {
                node.dispose();
                node.interaction_source = source.clone();
            }
        }
        Element::OnFocusChanged(callback) => {
            if let Some(node) = node.downcast_mut::<OnFocusChangedNode>() {
                node.callback = callback.clone();
            }
        }
        Element::OnKeyEvent(callback) => set!(OnKeyEventNode, OnKeyEventNode(callback.clone())),
        Element::TestTag(tag) => set!(TestTagNode, TestTagNode(tag.clone())),
        Element::Semantics(merge, properties) => {
            set!(SemanticsNode, SemanticsNode(*merge, properties.clone()))
        }
        Element::Custom(element) => element.update_dyn(node),
    }
}

/// A node with no capabilities.
pub(crate) struct Inert;

impl ModifierNode for Inert {}

/// What a slot holds while its node is taken out to run (see `UiTree::measure_layer`).
pub(crate) struct Taken;

impl ModifierNode for Taken {}

// ---- Layout ----------------------------------------------------------------------------------

struct PaddingNode(PaddingValues);

impl ModifierNode for PaddingNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for PaddingNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let PaddingValues { start, top, end, bottom } = self.0;
        let (start, top) = (density.round_to_px(start), density.round_to_px(top));
        let horizontal = start + density.round_to_px(end);
        let vertical = top + density.round_to_px(bottom);
        let placeable = scope.measure(content, constraints.offset(-horizontal, -vertical));
        let width = constraints.constrain_width(placeable.width + horizontal);
        let height = constraints.constrain_height(placeable.height + vertical);
        scope.place_relative(&placeable, start, top);
        scope.layout(width, height)
    }
}

struct OffsetNode(Dp, Dp);

impl ModifierNode for OffsetNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
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
        let density = scope.density();
        // In the layout direction, as `offset` (not `absoluteOffset`).
        scope.place_relative(&placeable, density.round_to_px(self.0), density.round_to_px(self.1));
        scope.layout(placeable.width, placeable.height)
    }
}

struct MinimumTouchTargetNode(Dp);

impl ModifierNode for MinimumTouchTargetNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for MinimumTouchTargetNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        let minimum = scope.density().round_to_px(self.0);
        let width = constraints.constrain_width(placeable.width.max(minimum));
        let height = constraints.constrain_height(placeable.height.max(minimum));
        let center = |space: i32| crate::unit::round_to_int(space as f32 / 2.0);
        scope.place(&placeable, center(width - placeable.width), center(height - placeable.height));
        scope.layout(width, height)
    }
}

struct SizeNode(SizeSpec);

impl SizeNode {
    /// As Compose's `SizeNode.targetConstraints`.
    fn target_constraints(&self, scope: &IntrinsicMeasureScope<'_>) -> Constraints {
        let density = scope.density();
        let spec = &self.0;
        let px = |dp: Dp| density.round_to_px(dp).max(0);
        let max_width =
            if spec.max_width.is_specified() { px(spec.max_width) } else { Constraints::INFINITY };
        let max_height = if spec.max_height.is_specified() {
            px(spec.max_height)
        } else {
            Constraints::INFINITY
        };
        let min = |dp: Dp, max: i32| {
            if dp.is_specified() {
                let value = px(dp).min(max);
                if value != Constraints::INFINITY { value } else { 0 }
            } else {
                0
            }
        };
        Constraints::new(
            min(spec.min_width, max_width),
            max_width,
            min(spec.min_height, max_height),
            max_height,
        )
    }
}

impl ModifierNode for SizeNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for SizeNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let target = self.target_constraints(scope);
        let spec = &self.0;
        let wrapped = if spec.enforce_incoming {
            constraints.constrain_by(target)
        } else {
            let min_width = if spec.min_width.is_specified() {
                target.min_width
            } else {
                constraints.min_width.min(target.max_width)
            };
            let max_width = if spec.max_width.is_specified() {
                target.max_width
            } else {
                constraints.max_width.max(target.min_width)
            };
            let min_height = if spec.min_height.is_specified() {
                target.min_height
            } else {
                constraints.min_height.min(target.max_height)
            };
            let max_height = if spec.max_height.is_specified() {
                target.max_height
            } else {
                constraints.max_height.max(target.min_height)
            };
            Constraints::new(min_width, max_width, min_height, max_height)
        };
        let placeable = scope.measure(content, wrapped);
        scope.place_relative(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }

    // As Compose's `SizeNode`: a fixed size is the intrinsic; otherwise the content's, asked
    // within the target (unless enforcing the incoming constraints) and kept within it.
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

impl SizeNode {
    fn intrinsic(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        kind: Intrinsic,
        value: i32,
    ) -> i32 {
        let target = self.target_constraints(scope);
        let enforce = self.0.enforce_incoming;
        match kind {
            Intrinsic::MinWidth | Intrinsic::MaxWidth => {
                if target.has_fixed_width() {
                    return target.max_width;
                }
                let height = if enforce { value } else { target.constrain_height(value) };
                target.constrain_width(scope.intrinsic(content, kind, height))
            }
            Intrinsic::MinHeight | Intrinsic::MaxHeight => {
                if target.has_fixed_height() {
                    return target.max_height;
                }
                let width = if enforce { value } else { target.constrain_width(value) };
                target.constrain_height(scope.intrinsic(content, kind, width))
            }
        }
    }
}

struct FillNode(Fill);

impl ModifierNode for FillNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for FillNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let axis = |fraction: Option<f32>, bounded: bool, min: i32, max: i32| match fraction {
            Some(fraction) if bounded => {
                let size = crate::unit::round_to_int(max as f32 * fraction).clamp(min, max);
                (size, size)
            }
            _ => (min, max),
        };
        let (min_width, max_width) = axis(
            self.0.horizontal,
            constraints.has_bounded_width(),
            constraints.min_width,
            constraints.max_width,
        );
        let (min_height, max_height) = axis(
            self.0.vertical,
            constraints.has_bounded_height(),
            constraints.min_height,
            constraints.max_height,
        );
        let placeable =
            scope.measure(content, Constraints::new(min_width, max_width, min_height, max_height));
        scope.place_relative(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

// ---- Drawing ---------------------------------------------------------------------------------

struct BackgroundNode(Color, Shape);

impl ModifierNode for BackgroundNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for BackgroundNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        if self.0.is_specified() {
            scope.draw_shape(&self.1, self.0);
        }
    }
}

// ---- Hoverable -------------------------------------------------------------------------------

/// `hoverable`'s element: a mouse over the node emitted into its source, as Compose's.
#[derive(Clone)]
pub(crate) struct HoverableElement {
    pub(crate) interaction_source: crate::interaction::MutableInteractionSource,
}

pub(crate) struct HoverableNode {
    interaction_source: crate::interaction::MutableInteractionSource,
    hover: Option<crate::interaction::HoverEnter>,
}

impl HoverableNode {
    /// The hover is over, as `tryEmitExit`.
    fn exit(&mut self) {
        if let Some(enter) = self.hover.take() {
            self.interaction_source.try_emit(crate::interaction::HoverInteraction::Exit(enter));
        }
    }
}

impl ModifierNodeElement for HoverableElement {
    type Node = HoverableNode;
    fn create(&self) -> HoverableNode {
        HoverableNode { interaction_source: self.interaction_source.clone(), hover: None }
    }
    fn update(&self, node: &mut HoverableNode) {
        if node.interaction_source != self.interaction_source {
            node.exit();
            node.interaction_source = self.interaction_source.clone();
        }
    }
    fn same(&self, other: &Self) -> bool {
        other.interaction_source == self.interaction_source
    }
    fn name(&self) -> &'static str {
        "hoverable"
    }
}

impl ModifierNode for HoverableNode {
    /// Its hover is over, as `HoverableNode.onDetach`'s `tryEmitExit`.
    fn on_detach(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.exit();
    }
    fn on_reset(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.exit();
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for HoverableNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        _context: &mut EventContext<'_>,
    ) {
        if pass != PointerEventPass::Main {
            return;
        }
        match event.kind {
            PointerEventType::Enter => {
                if self.hover.is_none() {
                    let enter = crate::interaction::HoverEnter::new();
                    self.interaction_source
                        .try_emit(crate::interaction::HoverInteraction::Enter(enter));
                    self.hover = Some(enter);
                }
            }
            PointerEventType::Exit => {
                if let Some(enter) = self.hover.take() {
                    self.interaction_source
                        .try_emit(crate::interaction::HoverInteraction::Exit(enter));
                }
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.exit();
    }
}

// ---- Focus, keys, semantics ------------------------------------------------------------------

struct FocusableNode {
    interaction_source: Option<crate::interaction::MutableInteractionSource>,
    focused: bool,
    /// The focus emitted into the source and not yet ended.
    focus: Option<crate::interaction::Focus>,
    /// Its emissions, in order (D1057).
    emitter: crate::interaction::Emitter,
    /// Its handle while it's attached, for its coroutine scope where it has no context.
    handle: Option<crate::node_context::NodeHandle>,
    /// Its coroutine scope, once asked for, while it's attached.
    scope: Option<compose_runtime::CoroutineScope>,
}

impl FocusableNode {
    /// It leaves (reset or detached): what it queued goes as its scope is cancelled (D1061), its
    /// focus interaction ends, as `disposeInteractionSource`.
    fn leave(&mut self) {
        self.emitter.drop_queued();
        self.scope = None;
        self.handle = None;
        self.dispose();
        self.focused = false;
    }

    /// The focus interaction ends, as `disposeInteractionSource`.
    fn dispose(&mut self) {
        if let Some(focus) = self.focus.take()
            && let Some(source) = &self.interaction_source
        {
            self.emitter.emit_now(source, crate::interaction::FocusInteraction::Unfocus(focus));
        }
    }
}

impl ModifierNode for FocusableNode {
    /// Asked to run its lifecycle, which the tree skips for the other built-ins
    /// (lifecycle.rs `for_custom_entries`).
    fn observes_reads(&self) -> bool {
        true
    }
    fn on_attach(&mut self, context: &mut crate::node_context::NodeContext<'_>) {
        self.handle = Some(context.handle());
    }
    /// As `FocusableNode.onDetach`.
    fn on_detach(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.leave();
    }
    /// As `FocusableNode.onReset`: detached next, attached again when reused.
    fn on_reset(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.leave();
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl FocusTargetModifierNode for FocusableNode {
    fn on_focus_changed(
        &mut self,
        _previous: crate::focus::FocusState,
        current: crate::focus::FocusState,
        context: &mut EventContext<'_>,
    ) {
        let focused = current.is_focused();
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        if focused {
            context.bring_into_view();
        }
        // As `FocusableNode.emitInteraction`.
        if let Some(source) = &self.interaction_source {
            let scope = match self.scope {
                Some(scope) => Some(scope),
                None => {
                    self.scope = self.handle.and_then(|handle| handle.coroutine_scope());
                    self.scope
                }
            };
            if let Some(focus) = self.focus.take() {
                self.emitter.emit_soon(
                    scope,
                    source,
                    crate::interaction::FocusInteraction::Unfocus(focus),
                );
            }
            if focused {
                let focus = crate::interaction::Focus::new();
                self.emitter.emit_soon(
                    scope,
                    source,
                    crate::interaction::FocusInteraction::Focus(focus),
                );
                self.focus = Some(focus);
            }
        }
        context.invalidate_draw();
    }
}

impl SemanticsModifierNode for FocusableNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.set_focused(self.focused);
        config.request_focus_of_node();
    }
}

/// `on_focus_changed`: its callback, and the state it last reported (to the application: what's
/// told it runs after the transaction, and doesn't if the focus moved meanwhile).
pub(crate) struct OnFocusChangedNode {
    pub(crate) callback: Callback<dyn Fn(crate::focus::FocusState)>,
    pub(crate) last: Rc<std::cell::Cell<Option<crate::focus::FocusState>>>,
}

impl ModifierNode for OnFocusChangedNode {
    /// Reused, it reports the new content's focus to the new callback.
    fn on_reset(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.last.set(None);
    }
    fn as_focus_event(&mut self) -> Option<&mut dyn crate::modifier::FocusEventModifierNode> {
        Some(self)
    }
}

struct OnKeyEventNode(Callback<dyn Fn(KeyEvent) -> bool>);

impl ModifierNode for OnKeyEventNode {
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
}

impl KeyInputModifierNode for OnKeyEventNode {
    /// The handler runs during dispatch, since its result decides where the event goes: it may
    /// write state, but not reach the tree.
    fn on_key_event(&mut self, event: &KeyEvent, _: &mut EventContext<'_>) -> bool {
        (self.0)(*event)
    }
}

struct TestTagNode(SharedString);

impl ModifierNode for TestTagNode {
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl SemanticsModifierNode for TestTagNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.set_test_tag(self.0.clone());
    }
}

struct SemanticsNode(bool, Rc<SemanticsConfiguration>);

impl ModifierNode for SemanticsNode {
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl SemanticsModifierNode for SemanticsNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.is_merging_semantics_of_descendants |= self.0;
        config.apply(&self.1);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use compose_runtime::platform::run_ready;

    use crate::focus_requester::{FocusRequest, FocusRequester};
    use crate::host::UiHost;
    use crate::interaction::{Interaction, MutableInteractionSource};
    use crate::lifecycle::Lifecycle;
    use crate::modifier::Modifier;
    use crate::tree::{NodeId, UiTree};

    /// The first layout node with a focusable.
    fn focusable_node(tree: &UiTree) -> NodeId {
        let mut stack = vec![tree.root_id()];
        while let Some(id) = stack.pop() {
            let chain = &tree.node(id).chain;
            if chain.iter().any(|entry| entry.node.downcast_ref::<super::FocusableNode>().is_some())
            {
                return id;
            }
            stack.extend(tree.children_of(id));
        }
        panic!("no focusable");
    }

    /// A focusable leaving with a focus queued sends it, then its end, as its scope is about to
    /// be cancelled (`drop_queued`, D1061), and attached again it emits: its queue starts a new
    /// task on its new scope.
    // Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Focusable.kt, emitWithFallback
    #[test]
    fn a_focusable_leaving_with_a_focus_queued_ends_it_and_emits_when_it_is_back() {
        let source = MutableInteractionSource::new();
        let heard = Rc::new(RefCell::new(Vec::new()));
        let sink = heard.clone();
        let _listener = source.listen(move |interaction| {
            sink.borrow_mut().push(match interaction {
                Interaction::Focus(crate::interaction::FocusInteraction::Focus(_)) => "Focus",
                Interaction::Focus(crate::interaction::FocusInteraction::Unfocus(_)) => "Unfocus",
                _ => "other",
            })
        });
        let requester = FocusRequester::new();
        let given = source.clone();
        let mut host = UiHost::headless(100, 100);
        host.set_content(move || {
            crate::layout::emit_layout_node(
                Modifier.focus_requester(requester).focusable_with(true, Some(given.clone())),
                crate::layout::empty_measure_policy(),
                || {},
            );
        });
        host.frame();
        assert!(requester.request_focus());
        host.handle_focus_requests();
        assert!(heard.borrow().is_empty(), "queued for the node's task");

        host.with_tree(|tree| {
            let node = focusable_node(tree);
            tree.reset_and_detach(node);
        });
        assert_eq!(*heard.borrow(), ["Focus", "Unfocus"]);
        assert!(!source.is_focused());
        run_ready();
        assert_eq!(*heard.borrow(), ["Focus", "Unfocus"], "nothing left for its task");

        host.with_tree(|tree| {
            let node = focusable_node(tree);
            tree.queue_lifecycle(Lifecycle::AttachEntries(node));
            tree.run_lifecycle();
            tree.focus_requests.push(FocusRequest::Clear { force: true });
        });
        host.handle_focus_requests();
        assert!(requester.request_focus());
        host.handle_focus_requests();
        run_ready();
        assert_eq!(*heard.borrow(), ["Focus", "Unfocus", "Focus"]);
        assert!(source.is_focused());
    }
}
