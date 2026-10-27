//! The nodes of the built-in modifier elements. Each keeps its element's data, updated in place
//! on recomposition, and its own state (pressed, focused).

use std::rc::Rc;

use compose_runtime::Callback;

use crate::constraints::Constraints;
use crate::draw::DrawScope;
use crate::graphics::{Color, Outline, Paint, Shape};
use crate::input::{
    EventContext, Key, KeyEvent, KeyEventType, PointerEvent, PointerEventPass, PointerEventType,
};
use crate::layout::{Intrinsic, IntrinsicMeasureScope, Measurable, MeasureResult, MeasureScope};
use crate::modifier::{
    Clickable, DrawModifierNode, Element, Fill, FocusTargetModifierNode, KeyInputModifierNode,
    LayoutModifierNode, ModifierNode, ModifierNodeElement, PaddingValues, PointerInputModifierNode,
    SemanticsModifierNode, SizeSpec, Toggleable,
};
use crate::semantics::{SemanticsConfiguration, ToggleableState};
use crate::text::SharedString;
use crate::unit::{Dp, IntSize, Rect};

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
        Element::Weight(..) | Element::Align(_) | Element::ZIndex(_) => Box::new(Inert),
        Element::Background(color, shape) => Box::new(BackgroundNode(*color, *shape)),
        Element::Border(width, color, shape) => Box::new(BorderNode(*width, *color, *shape)),
        // A graphics layer clipping to its shape, as Compose's `clip` (D523).
        Element::Clip(shape) => Box::new(crate::graphics_layer::GraphicsLayerNode::clip(*shape)),
        Element::Clickable(clickable) => Box::new(ClickableNode {
            spec: Press::Click(clickable.clone()),
            state: PressState::default(),
        }),
        Element::Toggleable(toggleable) => Box::new(ClickableNode {
            spec: Press::Toggle(toggleable.clone()),
            state: PressState::default(),
        }),
        Element::Focusable(enabled) => {
            Box::new(FocusableNode { enabled: *enabled, focused: false })
        }
        Element::OnFocusChanged(callback) => {
            Box::new(OnFocusChangedNode { callback: callback.clone(), last: None })
        }
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
        Element::Weight(..) | Element::Align(_) | Element::ZIndex(_) => {}
        Element::Background(color, shape) => set!(BackgroundNode, BackgroundNode(*color, *shape)),
        Element::Border(width, color, shape) => {
            set!(BorderNode, BorderNode(*width, *color, *shape))
        }
        Element::Clip(shape) => set!(
            crate::graphics_layer::GraphicsLayerNode,
            crate::graphics_layer::GraphicsLayerNode::clip(*shape)
        ),
        Element::Clickable(clickable) => {
            if let Some(node) = node.downcast_mut::<ClickableNode>() {
                // A source swapped (its press, focus and hover told over) or the clickable
                // disabled mid-press: the old press is over.
                let pressed = node.state.pressed.is_some() || node.state.keys_pressed != 0;
                let swapped =
                    node.spec.interaction_source() != clickable.interaction_source.as_ref();
                // Released on the old source, at the swap: the reset after the pass sees the
                // new one (`ModifierNode::on_reset`).
                if swapped || (pressed && !clickable.enabled) {
                    node.release_later();
                }
                node.spec = Press::Click(clickable.clone());
                node.state.disable_if(!clickable.enabled);
            }
        }
        Element::Toggleable(toggleable) => {
            if let Some(node) = node.downcast_mut::<ClickableNode>() {
                let pressed = node.state.pressed.is_some() || node.state.keys_pressed != 0;
                let swapped =
                    node.spec.interaction_source() != toggleable.interaction_source.as_ref();
                // Released on the old source, at the swap: the reset after the pass sees the
                // new one (`ModifierNode::on_reset`).
                if swapped || (pressed && !toggleable.enabled) {
                    node.release_later();
                }
                node.spec = Press::Toggle(toggleable.clone());
                node.state.disable_if(!toggleable.enabled);
            }
        }
        Element::Focusable(enabled) => {
            if let Some(node) = node.downcast_mut::<FocusableNode>() {
                node.enabled = *enabled;
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

struct BorderNode(Dp, Color, Shape);

impl ModifierNode for BorderNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for BorderNode {
    /// Over the content, as Compose's border, with the stroke inside the bounds: whole pixels
    /// (a hairline for 0 dp), and the shape filled when the stroke would meet itself.
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        if !self.1.is_specified() {
            return;
        }
        let width = if self.0.0 == 0.0 { 1.0 } else { scope.density().to_px(self.0).ceil() };
        let size = scope.size();
        if width * 2.0 >= size.width.min(size.height) {
            let outline = self.2.outline_in(size, &scope.density(), scope.layout_direction());
            scope.draw_outline(outline, Paint::fill(self.1));
            return;
        }
        let half = width / 2.0;
        let paint = Paint::stroke(self.1, width);
        match self.2.outline_in(size, &scope.density(), scope.layout_direction()) {
            Outline::Rectangle(rect) => scope.draw_rect(inset(rect, half), paint),
            Outline::Rounded(rect, radii) => scope.draw_round_rect(
                inset(rect, half),
                radii.map(|radius| (radius - half).max(0.0)),
                paint,
            ),
        }
    }
}

fn inset(rect: Rect, by: f32) -> Rect {
    Rect::new(rect.left + by, rect.top + by, rect.right - by, rect.bottom - by)
}

// ---- Clickable and toggleable ----------------------------------------------------------------

enum Press {
    Click(Clickable),
    Toggle(Toggleable),
}

impl Press {
    fn indication(&self) -> crate::modifier::Indication {
        match self {
            Press::Click(clickable) => clickable.indication,
            Press::Toggle(toggleable) => toggleable.indication,
        }
    }

    fn enabled(&self) -> bool {
        match self {
            Press::Click(clickable) => clickable.enabled,
            Press::Toggle(toggleable) => toggleable.enabled,
        }
    }

    fn interaction_source(&self) -> Option<&crate::interaction::MutableInteractionSource> {
        match self {
            Press::Click(clickable) => clickable.interaction_source.as_ref(),
            Press::Toggle(toggleable) => toggleable.interaction_source.as_ref(),
        }
    }

    fn fire(&self, context: &mut EventContext<'_>) {
        match self {
            Press::Click(clickable) => context.call(&clickable.on_click),
            Press::Toggle(toggleable) => {
                context.call_with_bool(&toggleable.on_value_change, !toggleable.value)
            }
        }
    }
}

#[derive(Default)]
struct PressState {
    /// The pointer of a press in progress, which its release inside completes.
    pressed: Option<crate::input::PointerId>,
    /// The keys (Enter, NumPadEnter, Space) pressed and not released yet, as bits.
    keys_pressed: u8,
    focused: bool,
    /// A mouse is over it.
    hovered: bool,
    /// A combined clickable's tap detection holds a press (it reports there).
    tap_pressed: Option<Rc<std::cell::Cell<bool>>>,
    /// A combined clickable's tap detection told its source a press (as `published`).
    tap_published: Option<Rc<std::cell::Cell<bool>>>,
    /// Its interaction source was told its press, focus or hover since it was last released:
    /// what releasing it tells over (a source it never told, swapped in, is left alone).
    published: bool,
}

fn key_bit(key: Key) -> u8 {
    match key {
        Key::Enter => 1,
        Key::NumPadEnter => 2,
        _ => 4,
    }
}

impl PressState {
    fn disable_if(&mut self, disabled: bool) {
        if disabled {
            self.pressed = None;
            self.keys_pressed = 0;
        }
    }
}

/// `clickable` and `toggleable`: a press released inside clicks, as Compose's
/// `detectTapGestures`; moving out, or another handler consuming the events, cancels. Focused,
/// Enter and Space click, as on desktop Compose.
struct ClickableNode {
    spec: Press,
    state: PressState,
}

impl ModifierNode for ClickableNode {
    /// Leaving mid-press releases its interaction source, as Compose's `disposeInteractions`.
    fn on_release(&mut self, _context: &mut crate::modifier::ReleaseContext<'_>) {
        self.release_later();
    }
    /// Reused, the same as leaving: its press, focus and hover are over.
    fn on_reset(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.release_later();
        let (tap_pressed, tap_published) =
            (self.state.tap_pressed.take(), self.state.tap_published.take());
        if let Some(pressed) = &tap_pressed {
            pressed.set(false);
        }
        self.state = PressState { tap_pressed, tap_published, ..PressState::default() };
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

/// Material's state layer opacity for pressed and focused, as its ripple settles at.
const STATE_LAYER_ALPHA: f32 = 0.10;
/// Material's hovered state layer.
const HOVER_STATE_LAYER_ALPHA: f32 = 0.08;

impl DrawModifierNode for ClickableNode {
    /// The state layer, over the content, while pressed or focused.
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        self.draw_indication(scope, false);
    }
}

impl ClickableNode {
    /// The state layer, while pressed (or `pressed` by something else), focused or hovered.
    fn draw_indication(&self, scope: &mut DrawScope<'_>, pressed: bool) {
        let indication = self.spec.indication();
        let active = pressed
            || self.state.pressed.is_some()
            || self.state.keys_pressed != 0
            || self.state.focused;
        // Material's state layers: pressed and focused 10 %, hovered 8 %.
        let alpha = if active {
            Some(STATE_LAYER_ALPHA)
        } else if self.state.hovered {
            Some(HOVER_STATE_LAYER_ALPHA)
        } else {
            None
        };
        if let Some(alpha) = alpha
            && indication.color.is_specified()
            && self.spec.enabled()
        {
            let bounds = scope.bounds();
            let paint = Paint::fill(indication.color.with_alpha(alpha));
            match indication.radius {
                None => scope.draw_rect(bounds, paint),
                Some(radius) => {
                    let radius = scope.density().to_px(radius);
                    let (x, y) =
                        ((bounds.left + bounds.right) / 2.0, (bounds.top + bounds.bottom) / 2.0);
                    let circle = Rect::new(x - radius, y - radius, x + radius, y + radius);
                    scope.draw_round_rect(circle, [radius; 4], paint);
                }
            }
        }
    }
}

fn inside(event: &PointerEvent, bounds: IntSize) -> bool {
    let position = event.position;
    position.x >= 0.0
        && position.y >= 0.0
        && position.x < bounds.width as f32
        && position.y < bounds.height as f32
}

impl ClickableNode {
    /// Its press changed: drawn again, and told to its interaction source.
    fn press_changed(&mut self, context: &mut EventContext<'_>) {
        context.invalidate_draw();
        self.publish_press();
    }

    /// Releases its interaction source after the composition applying a change (a state
    /// can't be written while composing): its press, focus and hover are over.
    fn release_later(&mut self) {
        let active = self.is_pressed() || self.state.focused || self.state.hovered;
        let tapped = self.state.tap_published.as_ref().is_some_and(|told| told.replace(false));
        let published = std::mem::take(&mut self.state.published) || tapped;
        if active
            && published
            && let Some(source) = self.spec.interaction_source().cloned()
        {
            compose_runtime::CoroutineScope::new().launch(async move {
                source.set_pressed(false);
                source.set_focused(false);
                source.set_hovered(false);
            });
        }
    }

    /// Pressed by a pointer, a key, or (a combined clickable) its tap detection.
    fn is_pressed(&self) -> bool {
        self.state.pressed.is_some()
            || self.state.keys_pressed != 0
            || self.state.tap_pressed.as_ref().is_some_and(|pressed| pressed.get())
    }

    fn publish_press(&mut self) {
        if let Some(source) = self.spec.interaction_source() {
            self.state.published = true;
            source.set_pressed(self.is_pressed());
            source.set_focused(self.state.focused);
            source.set_hovered(self.state.hovered);
        }
    }
}

impl PointerInputModifierNode for ClickableNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        // A move of its press that an ancestor took by the final pass (a drag around it)
        // cancels the press, as Compose's `waitForUpOrCancellation`.
        if pass == PointerEventPass::Final
            && event.kind == PointerEventType::Move
            && event.consumed
            && self.state.pressed == Some(event.pointer)
        {
            self.state.pressed = None;
            self.press_changed(context);
            return;
        }
        if pass != PointerEventPass::Main {
            return;
        }
        match event.kind {
            PointerEventType::Enter | PointerEventType::Exit => {
                let hovered = event.kind == PointerEventType::Enter;
                if self.state.hovered != hovered {
                    self.state.hovered = hovered;
                    self.press_changed(context);
                }
            }
            PointerEventType::Press => {
                if self.spec.enabled() && !event.consumed && self.state.pressed.is_none() {
                    self.state.pressed = Some(event.pointer);
                    event.consume();
                    self.press_changed(context);
                }
            }
            PointerEventType::Move => {
                if self.state.pressed == Some(event.pointer)
                    && (event.consumed || !inside(event, bounds))
                {
                    self.state.pressed = None;
                    self.press_changed(context);
                }
            }
            PointerEventType::Release => {
                if self.state.pressed == Some(event.pointer) {
                    self.state.pressed = None;
                    self.press_changed(context);
                    if !event.consumed && inside(event, bounds) {
                        event.consume();
                        self.spec.fire(context);
                    }
                }
            }
            PointerEventType::Scroll => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.state.pressed = None;
        self.publish_press();
    }
}

impl SemanticsModifierNode for ClickableNode {
    fn perform_action(
        &mut self,
        action: crate::semantics::SemanticsAction,
        context: &mut EventContext<'_>,
    ) -> bool {
        if action != crate::semantics::SemanticsAction::OnClick || !self.spec.enabled() {
            return false;
        }
        self.spec.fire(context);
        true
    }

    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.merge_descendants = true;
        config.clickable = true;
        config.focusable |= self.spec.enabled();
        config.focused |= self.state.focused;
        match &self.spec {
            Press::Click(clickable) => {
                config.disabled |= !clickable.enabled;
                if clickable.role.is_some() {
                    config.role = clickable.role;
                }
            }
            Press::Toggle(toggleable) => {
                config.disabled |= !toggleable.enabled;
                if toggleable.role.is_some() {
                    config.role = toggleable.role;
                }
                config.toggleable_state = Some(ToggleableState::from(toggleable.value));
            }
        }
    }
}

impl FocusTargetModifierNode for ClickableNode {
    fn can_focus(&self) -> bool {
        self.spec.enabled()
    }

    fn on_focus_changed(&mut self, focused: bool, context: &mut EventContext<'_>) {
        self.state.focused = focused;
        if !focused {
            self.state.keys_pressed = 0;
        }
        self.press_changed(context);
    }
}

impl KeyInputModifierNode for ClickableNode {
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        if !matches!(event.key, Key::Enter | Key::NumPadEnter | Key::Space) || !self.spec.enabled()
        {
            return false;
        }
        match event.kind {
            KeyEventType::KeyDown => {
                self.state.keys_pressed |= key_bit(event.key);
                self.press_changed(context);
                true
            }
            KeyEventType::KeyUp => {
                let bit = key_bit(event.key);
                if self.state.keys_pressed & bit != 0 {
                    self.state.keys_pressed &= !bit;
                    self.press_changed(context);
                    self.spec.fire(context);
                    true
                } else {
                    false
                }
            }
        }
    }
}

// ---- Combined clickable ----------------------------------------------------------------------

/// `combined_clickable`'s element: a clickable whose pointer presses go through tap detection
/// (double clicks and long presses take time), its keys, focus, hover and semantics as
/// `clickable`'s.
pub(crate) struct CombinedClickableElement {
    pub(crate) clickable: Clickable,
    pub(crate) on_long_click: Option<Callback<dyn Fn()>>,
    pub(crate) on_double_click: Option<Callback<dyn Fn()>>,
    /// What a long click performs `LongPress` feedback with (`LOCAL_HAPTIC_FEEDBACK`).
    pub(crate) haptic_feedback: Option<Rc<dyn crate::platform_services::HapticFeedback>>,
}

/// The callbacks the tap detection calls, updated in place.
struct CombinedHandlers {
    /// Where the tap detection's press is reported.
    interaction_source: Option<crate::interaction::MutableInteractionSource>,
    /// The tap detection holds a press, for the clickable's own reports.
    tap_pressed: Rc<std::cell::Cell<bool>>,
    /// The tap detection told the source a press (the clickable's `tap_published`).
    published: Rc<std::cell::Cell<bool>>,
    on_click: Callback<dyn Fn()>,
    on_long_click: Option<Callback<dyn Fn()>>,
    on_double_click: Option<Callback<dyn Fn()>>,
    haptic_feedback: Option<Rc<dyn crate::platform_services::HapticFeedback>>,
}

pub(crate) struct CombinedClickableNode {
    clickable: ClickableNode,
    handlers: Rc<std::cell::RefCell<CombinedHandlers>>,
    driver: crate::pointer_input::PointerInputDriver,
    /// Whether the tap detection holds a press: read while drawing, so its writes draw again.
    pressed: compose_runtime::OwnedState<bool>,
}

impl CombinedClickableNode {
    /// A tap detector for these handlers: whether there are a long and a double click decides
    /// how it waits.
    fn driver(
        handlers: &Rc<std::cell::RefCell<CombinedHandlers>>,
        pressed: compose_runtime::MutableState<bool>,
    ) -> crate::pointer_input::PointerInputDriver {
        let handlers = handlers.clone();
        crate::pointer_input::PointerInputDriver::new(move |scope| {
            let handlers = handlers.clone();
            async move {
                let (long, double) = {
                    let current = handlers.borrow();
                    (current.on_long_click.is_some(), current.on_double_click.is_some())
                };
                let mut gestures = crate::pointer_input::TapGestures::default();
                let on_click = handlers.clone();
                gestures = gestures.on_tap(move |_| {
                    let callback = on_click.borrow().on_click.clone();
                    callback();
                });
                if long {
                    let on_long = handlers.clone();
                    gestures = gestures.on_long_press(move |_| {
                        let (callback, haptics) = {
                            let current = on_long.borrow();
                            (current.on_long_click.clone(), current.haptic_feedback.clone())
                        };
                        if let Some(callback) = callback {
                            callback();
                            if let Some(haptics) = haptics {
                                haptics.perform_haptic_feedback(
                                    crate::platform_services::HapticFeedbackType::LongPress,
                                );
                            }
                        }
                    });
                }
                if double {
                    let on_double = handlers.clone();
                    gestures = gestures.on_double_tap(move |_| {
                        let callback = on_double.borrow().on_double_click.clone();
                        if let Some(callback) = callback {
                            callback();
                        }
                    });
                }
                let report = move |down: bool| {
                    if pressed.try_get() != Some(down) {
                        pressed.set(down);
                    }
                    let current = handlers.borrow();
                    current.tap_pressed.set(down);
                    if let Some(source) = &current.interaction_source {
                        source.set_pressed(down);
                        current.published.set(true);
                    }
                };
                scope.detect_taps(gestures, &report).await;
            }
        })
    }

    /// The press is over without its events (cancelled, disabled, detached).
    fn stop(&mut self) {
        self.driver.stop();
        if self.pressed.try_get() == Some(true) {
            // Disabled or detached by a recomposition: written once it's over (state can't
            // be written while composing).
            let pressed = self.pressed.handle();
            if compose_runtime::is_composing() {
                compose_runtime::CoroutineScope::new().launch(async move {
                    let _ = pressed.try_set(false);
                });
            } else {
                pressed.set(false);
            }
        }
    }
}

impl ModifierNodeElement for CombinedClickableElement {
    type Node = CombinedClickableNode;
    fn create(&self) -> CombinedClickableNode {
        let tap_pressed = Rc::new(std::cell::Cell::new(false));
        let tap_published = Rc::new(std::cell::Cell::new(false));
        let handlers = Rc::new(std::cell::RefCell::new(CombinedHandlers {
            interaction_source: self.clickable.interaction_source.clone(),
            tap_pressed: tap_pressed.clone(),
            published: tap_published.clone(),
            on_click: self.clickable.on_click.clone(),
            on_long_click: self.on_long_click.clone(),
            on_double_click: self.on_double_click.clone(),
            haptic_feedback: self.haptic_feedback.clone(),
        }));
        let pressed = compose_runtime::mutable_state_of(false);
        CombinedClickableNode {
            clickable: ClickableNode {
                spec: Press::Click(self.clickable.clone()),
                state: PressState {
                    tap_pressed: Some(tap_pressed),
                    tap_published: Some(tap_published),
                    ..PressState::default()
                },
            },
            driver: CombinedClickableNode::driver(&handlers, pressed.handle()),
            handlers,
            pressed,
        }
    }

    fn update(&self, node: &mut CombinedClickableNode) {
        let mut handlers = node.handlers.borrow_mut();
        // Another kind of gesture to wait for: the detection starts over.
        let restart = handlers.on_long_click.is_some() != self.on_long_click.is_some()
            || handlers.on_double_click.is_some() != self.on_double_click.is_some();
        let swapped = handlers.interaction_source != self.clickable.interaction_source;
        handlers.interaction_source = self.clickable.interaction_source.clone();
        handlers.on_click = self.clickable.on_click.clone();
        handlers.on_long_click = self.on_long_click.clone();
        handlers.on_double_click = self.on_double_click.clone();
        handlers.haptic_feedback = self.haptic_feedback.clone();
        drop(handlers);
        if swapped {
            node.clickable.release_later();
        }
        if restart || swapped || !self.clickable.enabled {
            node.stop();
        }
        node.clickable.spec = Press::Click(self.clickable.clone());
        node.clickable.state.disable_if(!self.clickable.enabled);
    }

    fn same(&self, other: &Self) -> bool {
        let same_callback =
            |a: &Option<Callback<dyn Fn()>>, b: &Option<Callback<dyn Fn()>>| match (a, b) {
                (Some(a), Some(b)) => a.ptr_eq(b),
                (None, None) => true,
                _ => false,
            };
        {
            let (a, b) = (&self.clickable, &other.clickable);
            a.enabled == b.enabled
                && a.role == b.role
                && a.indication == b.indication
                && a.interaction_source == b.interaction_source
                && a.on_click.ptr_eq(&b.on_click)
                && same_callback(&self.on_long_click, &other.on_long_click)
                && same_callback(&self.on_double_click, &other.on_double_click)
                && match (&self.haptic_feedback, &other.haptic_feedback) {
                    (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                    (None, None) => true,
                    _ => false,
                }
        }
    }

    fn name(&self) -> &'static str {
        "combined_clickable"
    }
}

impl ModifierNode for CombinedClickableNode {
    /// Leaving mid-press releases its interaction source, as a clickable's (its tap
    /// detection's press too).
    fn on_release(&mut self, _context: &mut crate::modifier::ReleaseContext<'_>) {
        self.clickable.release_later();
        self.stop();
    }
    fn on_reset(&mut self, context: &mut crate::node_context::NodeContext<'_>) {
        self.stop();
        self.clickable.on_reset(context);
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(&mut self.clickable)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(&mut self.clickable)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl SemanticsModifierNode for CombinedClickableNode {
    /// A clickable's, with a long-click action if it long-clicks.
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        self.clickable.apply_semantics(config);
        config.long_clickable |=
            self.handlers.borrow().on_long_click.is_some() && self.clickable.spec.enabled();
    }

    fn perform_action(
        &mut self,
        action: crate::semantics::SemanticsAction,
        context: &mut EventContext<'_>,
    ) -> bool {
        if action != crate::semantics::SemanticsAction::OnLongClick {
            return self.clickable.perform_action(action, context);
        }
        let on_long_click = self.handlers.borrow().on_long_click.clone();
        match on_long_click {
            Some(callback) if self.clickable.spec.enabled() => {
                context.call(&callback);
                true
            }
            _ => false,
        }
    }
}

impl DrawModifierNode for CombinedClickableNode {
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        let pressed = self.pressed.get();
        self.clickable.draw_indication(scope, pressed);
    }
}

impl PointerInputModifierNode for CombinedClickableNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        match event.kind {
            // The hover, as a clickable's.
            PointerEventType::Enter | PointerEventType::Exit => {
                self.clickable.on_pointer_event(event, pass, bounds, context)
            }
            _ if self.clickable.spec.enabled() => {
                self.driver.on_pointer_event(event, pass, bounds, context)
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.stop();
    }
}

// ---- Hoverable -------------------------------------------------------------------------------

/// `hoverable`'s element and node in one: whether a mouse is over it, reported to its source.
#[derive(Clone)]
pub(crate) struct HoverableElement {
    pub(crate) interaction_source: crate::interaction::MutableInteractionSource,
    pub(crate) enabled: bool,
}

pub(crate) struct HoverableNode {
    spec: HoverableElement,
    hovered: bool,
}

impl HoverableNode {
    /// The hover is over: told to its source after the pass (a source can't be written while
    /// composing).
    fn exit(&mut self) {
        if std::mem::take(&mut self.hovered) {
            let source = self.spec.interaction_source.clone();
            compose_runtime::CoroutineScope::new().launch(async move { source.set_hovered(false) });
        }
    }
}

impl ModifierNodeElement for HoverableElement {
    type Node = HoverableNode;
    fn create(&self) -> HoverableNode {
        HoverableNode { spec: self.clone(), hovered: false }
    }
    fn update(&self, node: &mut HoverableNode) {
        if !self.enabled || node.spec.interaction_source != self.interaction_source {
            node.exit();
        }
        node.spec = self.clone();
    }
    fn same(&self, other: &Self) -> bool {
        other.enabled == self.enabled && other.interaction_source == self.interaction_source
    }
    fn name(&self) -> &'static str {
        "hoverable"
    }
}

impl ModifierNode for HoverableNode {
    fn on_release(&mut self, _context: &mut crate::modifier::ReleaseContext<'_>) {
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
        if pass != PointerEventPass::Main || !self.spec.enabled {
            return;
        }
        let hovered = match event.kind {
            PointerEventType::Enter => true,
            PointerEventType::Exit => false,
            _ => return,
        };
        if self.hovered != hovered {
            self.hovered = hovered;
            self.spec.interaction_source.set_hovered(hovered);
        }
    }
}

// ---- Focus, keys, semantics ------------------------------------------------------------------

struct FocusableNode {
    enabled: bool,
    focused: bool,
}

impl ModifierNode for FocusableNode {
    fn on_reset(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.focused = false;
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl FocusTargetModifierNode for FocusableNode {
    fn can_focus(&self) -> bool {
        self.enabled
    }

    fn on_focus_changed(&mut self, focused: bool, context: &mut EventContext<'_>) {
        self.focused = focused;
        context.invalidate_draw();
    }
}

impl SemanticsModifierNode for FocusableNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.focusable |= self.enabled;
        config.focused |= self.focused;
    }
}

/// `on_focus_changed`: its callback, and the state it last reported.
pub(crate) struct OnFocusChangedNode {
    pub(crate) callback: Callback<dyn Fn(bool)>,
    pub(crate) last: Option<bool>,
}

impl ModifierNode for OnFocusChangedNode {
    /// Reused, it reports the new content's focus to the new callback.
    fn on_reset(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.last = None;
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
        config.test_tag = Some(self.0.clone());
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
        config.merge_descendants |= self.0;
        config.apply(&self.1);
    }
}
