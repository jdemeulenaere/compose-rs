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
use crate::layout::{Measurable, MeasureResult, MeasureScope};
use crate::modifier::{
    Clickable, DrawModifierNode, Element, Fill, FocusTargetModifierNode, KeyInputModifierNode,
    LayoutModifierNode, ModifierNode, PaddingValues, PointerInputModifierNode,
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
        Element::Weight(..) | Element::Align(_) => Box::new(Inert),
        Element::Background(color, shape) => Box::new(BackgroundNode(*color, *shape)),
        Element::Border(width, color, shape) => Box::new(BorderNode(*width, *color, *shape)),
        Element::Clip(shape) => Box::new(ClipNode(*shape)),
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
        Element::Custom(element) => element.create(),
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
        Element::Weight(..) | Element::Align(_) => {}
        Element::Background(color, shape) => set!(BackgroundNode, BackgroundNode(*color, *shape)),
        Element::Border(width, color, shape) => {
            set!(BorderNode, BorderNode(*width, *color, *shape))
        }
        Element::Clip(shape) => set!(ClipNode, ClipNode(*shape)),
        Element::Clickable(clickable) => {
            if let Some(node) = node.downcast_mut::<ClickableNode>() {
                // A source swapped or the clickable disabled mid-press: the old press is over.
                let pressed = node.state.pressed.is_some() || node.state.keys_pressed != 0;
                let swapped = match &node.spec {
                    Press::Click(old) => old.interaction_source != clickable.interaction_source,
                    Press::Toggle(_) => false,
                };
                if pressed && (swapped || !clickable.enabled) {
                    node.release_later();
                }
                node.spec = Press::Click(clickable.clone());
                node.state.disable_if(!clickable.enabled);
            }
        }
        Element::Toggleable(toggleable) => {
            if let Some(node) = node.downcast_mut::<ClickableNode>() {
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
        Element::Custom(element) => element.update(node),
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
    fn target_constraints(&self, scope: &MeasureScope<'_>) -> Constraints {
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
            let outline = self.2.outline(size, &scope.density());
            scope.draw_outline(outline, Paint::fill(self.1));
            return;
        }
        let half = width / 2.0;
        let paint = Paint::stroke(self.1, width);
        match self.2.outline(size, &scope.density()) {
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

pub(crate) struct ClipNode(pub(crate) Shape);

impl ModifierNode for ClipNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn hit_clip(&self) -> Option<Shape> {
        Some(self.0)
    }
}

impl DrawModifierNode for ClipNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        scope.save();
        let outline = self.0.outline(scope.size(), &scope.density());
        scope.clip_outline(outline);
    }

    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        scope.restore();
    }
}

// ---- Clickable and toggleable ----------------------------------------------------------------

enum Press {
    Click(Clickable),
    Toggle(Toggleable),
}

impl Press {
    fn indication(&self) -> Color {
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
    fn on_detach(&mut self, _context: &mut crate::modifier::DetachContext<'_>) {
        self.release_later();
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

impl DrawModifierNode for ClickableNode {
    /// The state layer, over the content, while pressed or focused.
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        let indication = self.spec.indication();
        let active =
            self.state.pressed.is_some() || self.state.keys_pressed != 0 || self.state.focused;
        if indication.is_specified() && active && self.spec.enabled() {
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(indication.with_alpha(STATE_LAYER_ALPHA)));
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
    fn press_changed(&self, context: &mut EventContext<'_>) {
        context.invalidate_draw();
        self.publish_press();
    }

    /// Releases its interaction source after the composition applying a change (a state
    /// can't be written while composing): its press is over.
    fn release_later(&self) {
        let pressed = self.state.pressed.is_some() || self.state.keys_pressed != 0;
        if pressed
            && let Press::Click(clickable) = &self.spec
            && let Some(source) = clickable.interaction_source.clone()
        {
            compose_runtime::CoroutineScope::new().launch(async move { source.set_pressed(false) });
        }
    }

    fn publish_press(&self) {
        if let Press::Click(clickable) = &self.spec
            && let Some(source) = &clickable.interaction_source
        {
            source.set_pressed(self.state.pressed.is_some() || self.state.keys_pressed != 0);
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
        if pass != PointerEventPass::Main {
            return;
        }
        match event.kind {
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

// ---- Focus, keys, semantics ------------------------------------------------------------------

struct FocusableNode {
    enabled: bool,
    focused: bool,
}

impl ModifierNode for FocusableNode {
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

impl ModifierNode for OnFocusChangedNode {}

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
