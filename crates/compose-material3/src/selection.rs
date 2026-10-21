//! Selection controls, as Compose's: `Switch`, `RadioButton` and `Slider`.

use compose_animation::{Animatable, spring, tween};
use compose_runtime::{Callback, CoroutineScope, composable};
use compose_ui::{
    Color, Constraints, DrawModifierNode, DrawScope, EventContext, FocusTargetModifierNode,
    IntSize, Key, KeyEvent, KeyEventType, KeyInputModifierNode, LayoutModifierNode, Measurable,
    MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement, Offset, Paint,
    PointerEvent, PointerEventPass, PointerEventType, PointerInputModifierNode, Rect, Role,
    SemanticsConfiguration, SemanticsModifierNode, UnitExt,
};

use crate::components::minimum_interactive_component_size;
use crate::theme::MaterialTheme;

/// A disabled control's alpha, as Material's.
const DISABLED_ALPHA: f32 = 0.38;

// ---- Switch ----------------------------------------------------------------------------------

/// A switch's colors, as Compose's `SwitchColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SwitchColors {
    pub checked_thumb_color: Color,
    pub checked_track_color: Color,
    pub unchecked_thumb_color: Color,
    pub unchecked_track_color: Color,
    pub unchecked_border_color: Color,
}

impl compose_runtime::ComposeArg for SwitchColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `SwitchDefaults`.
pub struct SwitchDefaults;

impl SwitchDefaults {
    /// From the theme, as `SwitchDefaults.colors()`.
    pub fn colors() -> SwitchColors {
        let scheme = MaterialTheme::color_scheme();
        SwitchColors {
            checked_thumb_color: scheme.on_primary,
            checked_track_color: scheme.primary,
            unchecked_thumb_color: scheme.outline,
            unchecked_track_color: scheme.surface_container_highest,
            unchecked_border_color: scheme.outline,
        }
    }
}

/// A switch, as Compose's: a 52 × 32 dp track whose thumb slides (animated) to the end when
/// checked, in a 48 dp touch target, toggled by a click when `on_checked_change` is given.
#[composable]
pub fn Switch(
    checked: bool,
    on_checked_change: Option<impl Fn(bool) + 'static>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::SwitchDefaults::colors())] colors: SwitchColors,
) {
    let modifier = match on_checked_change {
        Some(on_checked_change) => {
            minimum_interactive_component_size(modifier.toggleable_indicated(
                checked,
                enabled,
                Some(Role::Switch),
                if checked { colors.checked_track_color } else { colors.unchecked_thumb_color },
                on_checked_change,
            ))
        }
        None => modifier,
    };
    compose_foundation::layout::Spacer(
        modifier.required_size(52.dp(), 32.dp()).then_custom(SwitchElement {
            checked,
            enabled,
            colors,
        }),
    );
}

struct SwitchElement {
    checked: bool,
    enabled: bool,
    colors: SwitchColors,
}

struct SwitchNode {
    checked: bool,
    enabled: bool,
    colors: SwitchColors,
    /// 0 unchecked, 1 checked: the thumb's position and size.
    progress: Animatable<f32>,
    scope: CoroutineScope,
}

impl ModifierNodeElement for SwitchElement {
    fn create(&self) -> std::boxed::Box<dyn ModifierNode> {
        std::boxed::Box::new(SwitchNode {
            checked: self.checked,
            enabled: self.enabled,
            colors: self.colors,
            progress: Animatable::new(if self.checked { 1.0 } else { 0.0 }),
            scope: CoroutineScope::new(),
        })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        let Some(node) = node.downcast_mut::<SwitchNode>() else { return };
        if node.checked != self.checked {
            let (progress, target) = (node.progress.clone(), if self.checked { 1.0 } else { 0.0 });
            node.scope.launch(async move {
                progress.animate_to(target, spring()).await;
            });
        }
        node.checked = self.checked;
        node.enabled = self.enabled;
        node.colors = self.colors;
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<SwitchElement>().is_some_and(|other| {
            other.checked == self.checked
                && other.enabled == self.enabled
                && other.colors == self.colors
        })
    }
    fn name(&self) -> &'static str {
        "switch"
    }
}

impl ModifierNode for SwitchNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_detach(&mut self, _context: &mut compose_ui::DetachContext<'_>) {
        self.scope.cancel();
    }
}

fn blend(from: Color, to: Color, fraction: f32) -> Color {
    let channel =
        |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * fraction).round() as u32;
    Color::from_argb(
        channel(from.alpha(), to.alpha()) << 24
            | channel(from.red(), to.red()) << 16
            | channel(from.green(), to.green()) << 8
            | channel(from.blue(), to.blue()),
    )
}

impl DrawModifierNode for SwitchNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        // Read while drawing: each animation frame redraws the switch only.
        let progress = self.progress.value().clamp(0.0, 1.0);
        let size = scope.size();
        let density = scope.density();
        let colors = self.colors;
        let alpha =
            |color: Color| if self.enabled { color } else { color.with_alpha(DISABLED_ALPHA) };
        let track = Rect::new(0.0, 0.0, size.width, size.height);
        let radius = size.height / 2.0;
        let track_color = blend(colors.unchecked_track_color, colors.checked_track_color, progress);
        scope.draw_round_rect(track, [radius; 4], Paint::fill(alpha(track_color)));
        if progress < 1.0 {
            let border = density.to_px(2.dp());
            let half = border / 2.0;
            let inset = Rect::new(half, half, size.width - half, size.height - half);
            let border_color = colors.unchecked_border_color.with_alpha(1.0 - progress);
            scope.draw_round_rect(
                inset,
                [radius - half; 4],
                Paint::stroke(alpha(border_color), border),
            );
        }
        // The thumb: 16 dp unchecked, 24 dp checked, centered 16 dp from its end.
        let diameter = density.to_px(16.dp()) + density.to_px(8.dp()) * progress;
        let start = radius;
        let end = size.width - radius;
        let center = Offset::new(start + (end - start) * progress, size.height / 2.0);
        let thumb_color = blend(colors.unchecked_thumb_color, colors.checked_thumb_color, progress);
        let half = diameter / 2.0;
        scope.draw_round_rect(
            Rect::new(center.x - half, center.y - half, center.x + half, center.y + half),
            [half; 4],
            Paint::fill(alpha(thumb_color)),
        );
    }
}

// ---- RadioButton -----------------------------------------------------------------------------

/// A radio button's colors, as Compose's `RadioButtonColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RadioButtonColors {
    pub selected_color: Color,
    pub unselected_color: Color,
}

impl compose_runtime::ComposeArg for RadioButtonColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `RadioButtonDefaults`.
pub struct RadioButtonDefaults;

impl RadioButtonDefaults {
    pub fn colors() -> RadioButtonColors {
        let scheme = MaterialTheme::color_scheme();
        RadioButtonColors {
            selected_color: scheme.primary,
            unselected_color: scheme.on_surface_variant,
        }
    }
}

/// A radio button, as Compose's: a 20 dp ring, its dot growing in (animated) when selected, in
/// a 48 dp touch target, clicked when `on_click` is given.
#[composable]
pub fn RadioButton(
    selected: bool,
    on_click: Option<impl Fn() + 'static>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::RadioButtonDefaults::colors())] colors: RadioButtonColors,
) {
    let color = if selected { colors.selected_color } else { colors.unselected_color };
    let modifier = match on_click {
        Some(on_click) => minimum_interactive_component_size(modifier.clickable_indicated(
            enabled,
            Some(Role::RadioButton),
            color,
            on_click,
        )),
        None => modifier,
    };
    let modifier = modifier
        .semantics(false, move |config| config.selected = Some(selected))
        .padding(2.dp())
        .required_size(20.dp(), 20.dp())
        .then_custom(RadioElement { selected, enabled, colors });
    compose_foundation::layout::Spacer(modifier);
}

struct RadioElement {
    selected: bool,
    enabled: bool,
    colors: RadioButtonColors,
}

struct RadioNode {
    selected: bool,
    enabled: bool,
    colors: RadioButtonColors,
    /// The dot's radius in dp: 6 selected, 0 not.
    dot: Animatable<f32>,
    scope: CoroutineScope,
}

impl ModifierNodeElement for RadioElement {
    fn create(&self) -> std::boxed::Box<dyn ModifierNode> {
        std::boxed::Box::new(RadioNode {
            selected: self.selected,
            enabled: self.enabled,
            colors: self.colors,
            dot: Animatable::new(if self.selected { 6.0 } else { 0.0 }),
            scope: CoroutineScope::new(),
        })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        let Some(node) = node.downcast_mut::<RadioNode>() else { return };
        if node.selected != self.selected {
            let (dot, target) = (node.dot.clone(), if self.selected { 6.0 } else { 0.0 });
            // As Compose's `RadioAnimationDuration`.
            node.scope.launch(async move {
                dot.animate_to(target, tween(100)).await;
            });
        }
        node.selected = self.selected;
        node.enabled = self.enabled;
        node.colors = self.colors;
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<RadioElement>().is_some_and(|other| {
            other.selected == self.selected
                && other.enabled == self.enabled
                && other.colors == self.colors
        })
    }
    fn name(&self) -> &'static str {
        "radio_button"
    }
}

impl ModifierNode for RadioNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_detach(&mut self, _context: &mut compose_ui::DetachContext<'_>) {
        self.scope.cancel();
    }
}

impl DrawModifierNode for RadioNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let size = scope.size();
        let density = scope.density();
        let color =
            if self.selected { self.colors.selected_color } else { self.colors.unselected_color };
        let color = if self.enabled { color } else { color.with_alpha(DISABLED_ALPHA) };
        let stroke = density.to_px(2.dp());
        let center = Offset::new(size.width / 2.0, size.height / 2.0);
        let ring = size.width.min(size.height) / 2.0 - stroke / 2.0;
        scope.draw_round_rect(
            Rect::new(center.x - ring, center.y - ring, center.x + ring, center.y + ring),
            [ring; 4],
            Paint::stroke(color, stroke),
        );
        let dot = density.to_px(compose_ui::Dp(self.dot.value().max(0.0)));
        if dot > 0.0 {
            scope.draw_round_rect(
                Rect::new(center.x - dot, center.y - dot, center.x + dot, center.y + dot),
                [dot; 4],
                Paint::fill(color),
            );
        }
    }
}

// ---- Slider ----------------------------------------------------------------------------------

/// A slider's colors, as Compose's `SliderColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SliderColors {
    pub thumb_color: Color,
    pub active_track_color: Color,
    pub inactive_track_color: Color,
}

impl compose_runtime::ComposeArg for SliderColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `SliderDefaults`.
pub struct SliderDefaults;

impl SliderDefaults {
    pub fn colors() -> SliderColors {
        let scheme = MaterialTheme::color_scheme();
        SliderColors {
            thumb_color: scheme.primary,
            active_track_color: scheme.primary,
            inactive_track_color: scheme.surface_container_highest,
        }
    }
}

/// A slider, as Compose's `Slider`: a value in `value_range` picked by pressing or dragging along
/// the track (snapping to `steps` stops between the ends, if any) or with the arrow keys when
/// focused. It is as wide as it may be and 48 dp high; `on_value_change_finished` hears the end
/// of a gesture.
#[composable]
pub fn Slider(
    value: f32,
    on_value_change: impl Fn(f32) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default((0.0, 1.0))] value_range: (f32, f32),
    #[default(0)] steps: u32,
    #[default(None)] on_value_change_finished: Option<impl Fn() + 'static>,
    #[default(crate::SliderDefaults::colors())] colors: SliderColors,
) {
    let element = SliderElement {
        value,
        on_value_change,
        on_value_change_finished,
        enabled,
        range: value_range,
        steps,
        colors,
    };
    compose_foundation::layout::Spacer(modifier.then_custom(element));
}

struct SliderElement {
    value: f32,
    on_value_change: Callback<dyn Fn(f32)>,
    on_value_change_finished: Option<Callback<dyn Fn()>>,
    enabled: bool,
    range: (f32, f32),
    steps: u32,
    colors: SliderColors,
}

struct SliderNode {
    value: f32,
    on_value_change: Callback<dyn Fn(f32)>,
    on_value_change_finished: Option<Callback<dyn Fn()>>,
    enabled: bool,
    range: (f32, f32),
    steps: u32,
    colors: SliderColors,
    dragging: bool,
    focused: bool,
    /// The thumb's radius in pixels, from the last measurement.
    radius: f32,
}

impl ModifierNodeElement for SliderElement {
    fn create(&self) -> std::boxed::Box<dyn ModifierNode> {
        std::boxed::Box::new(SliderNode {
            value: self.value,
            on_value_change: self.on_value_change.clone(),
            on_value_change_finished: self.on_value_change_finished.clone(),
            enabled: self.enabled,
            range: self.range,
            steps: self.steps,
            colors: self.colors,
            dragging: false,
            focused: false,
            radius: 10.0,
        })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        let Some(node) = node.downcast_mut::<SliderNode>() else { return };
        node.value = self.value;
        node.on_value_change = self.on_value_change.clone();
        node.on_value_change_finished = self.on_value_change_finished.clone();
        node.enabled = self.enabled;
        node.range = self.range;
        node.steps = self.steps;
        node.colors = self.colors;
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<SliderElement>().is_some_and(|other| {
            other.value == self.value
                && other.on_value_change.ptr_eq(&self.on_value_change)
                && match (&other.on_value_change_finished, &self.on_value_change_finished) {
                    (Some(a), Some(b)) => a.ptr_eq(b),
                    (None, None) => true,
                    _ => false,
                }
                && other.enabled == self.enabled
                && other.range == self.range
                && other.steps == self.steps
                && other.colors == self.colors
        })
    }
    fn name(&self) -> &'static str {
        "slider"
    }
}

/// The thumb's diameter, as Compose's (20 dp).
const THUMB: compose_ui::Dp = compose_ui::Dp(20.0);

impl SliderNode {
    fn fraction(&self) -> f32 {
        let (start, end) = self.range;
        if end <= start { 0.0 } else { ((self.value - start) / (end - start)).clamp(0.0, 1.0) }
    }

    /// The value at `fraction` of the range, snapped to the steps.
    fn value_at(&self, fraction: f32) -> f32 {
        let mut fraction = fraction.clamp(0.0, 1.0);
        if self.steps > 0 {
            let intervals = (self.steps + 1) as f32;
            fraction = (fraction * intervals).round() / intervals;
        }
        let (start, end) = self.range;
        start + (end - start) * fraction
    }

    fn change_to(&mut self, value: f32, context: &mut EventContext<'_>) {
        if value != self.value {
            self.value = value;
            context.invalidate_draw();
            let callback = self.on_value_change.clone();
            context.run(move || callback(value));
        }
    }

    fn finish(&self, context: &mut EventContext<'_>) {
        if let Some(callback) = &self.on_value_change_finished {
            context.call(callback);
        }
    }
}

impl ModifierNode for SliderNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for SliderNode {
    /// As wide as allowed (at least a thumb), 48 dp high.
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let thumb = density.round_to_px(THUMB);
        let width = if constraints.max_width == Constraints::INFINITY {
            constraints.min_width.max(thumb)
        } else {
            constraints.max_width.max(constraints.min_width)
        };
        let size = constraints.constrain(IntSize::new(width, density.round_to_px(48.dp())));
        self.radius = density.to_px(THUMB) / 2.0;
        let placeable = scope.measure(content, Constraints::fixed(size.width, size.height));
        scope.place(&placeable, 0, 0);
        scope.layout(size.width, size.height)
    }
}

impl DrawModifierNode for SliderNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let size = scope.size();
        let density = scope.density();
        let alpha =
            |color: Color| if self.enabled { color } else { color.with_alpha(DISABLED_ALPHA) };
        let radius = density.to_px(THUMB) / 2.0;
        let track = density.to_px(4.dp());
        let center_y = size.height / 2.0;
        let (start, end) = (radius, (size.width - radius).max(radius));
        let x = start + (end - start) * self.fraction();
        let half = track / 2.0;
        scope.draw_round_rect(
            Rect::new(start, center_y - half, end, center_y + half),
            [half; 4],
            Paint::fill(alpha(self.colors.inactive_track_color)),
        );
        scope.draw_round_rect(
            Rect::new(start, center_y - half, x, center_y + half),
            [half; 4],
            Paint::fill(alpha(self.colors.active_track_color)),
        );
        if self.focused || self.dragging {
            // The state layer around the thumb, as Material's.
            let layer = radius * 2.0;
            scope.draw_round_rect(
                Rect::new(x - layer, center_y - layer, x + layer, center_y + layer),
                [layer; 4],
                Paint::fill(self.colors.thumb_color.with_alpha(0.1)),
            );
        }
        scope.draw_round_rect(
            Rect::new(x - radius, center_y - radius, x + radius, center_y + radius),
            [radius; 4],
            Paint::fill(alpha(self.colors.thumb_color)),
        );
    }
}

impl PointerInputModifierNode for SliderNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass != PointerEventPass::Main || !self.enabled {
            return;
        }
        let radius = self.radius;
        let span = (bounds.width as f32 - 2.0 * radius).max(1.0);
        let fraction = (event.position.x - radius) / span;
        match event.kind {
            PointerEventType::Press if !event.consumed => {
                event.consume();
                self.dragging = true;
                context.request_focus();
                let value = self.value_at(fraction);
                self.change_to(value, context);
            }
            PointerEventType::Move if self.dragging => {
                event.consume();
                let value = self.value_at(fraction);
                self.change_to(value, context);
            }
            PointerEventType::Release if self.dragging => {
                event.consume();
                self.dragging = false;
                context.invalidate_draw();
                self.finish(context);
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.dragging = false;
    }
}

impl FocusTargetModifierNode for SliderNode {
    fn can_focus(&self) -> bool {
        self.enabled
    }
    fn on_focus_changed(&mut self, focused: bool, context: &mut EventContext<'_>) {
        self.focused = focused;
        context.invalidate_draw();
    }
}

impl KeyInputModifierNode for SliderNode {
    /// The arrows move a step (or a hundredth), Home and End to the ends.
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        if event.kind != KeyEventType::KeyDown || !self.enabled {
            return false;
        }
        let step = if self.steps > 0 { 1.0 / (self.steps + 1) as f32 } else { 0.01 };
        let fraction = self.fraction();
        let target = match event.key {
            Key::DirectionRight | Key::DirectionUp => fraction + step,
            Key::DirectionLeft | Key::DirectionDown => fraction - step,
            Key::MoveHome => 0.0,
            Key::MoveEnd => 1.0,
            _ => return false,
        };
        let value = self.value_at(target);
        self.change_to(value, context);
        self.finish(context);
        true
    }
}

impl SemanticsModifierNode for SliderNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.focusable |= self.enabled;
        config.focused |= self.focused;
        config.disabled |= !self.enabled;
        config.state_description = Some(format!("{}", self.value).into());
    }
}
