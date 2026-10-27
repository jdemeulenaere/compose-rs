//! Selection controls, as Compose's: `Switch`, `RadioButton` and `Slider`.

use compose_animation::{Animatable, spring, tween};
use compose_runtime::{Callback, CoroutineScope, composable};
use compose_ui::ClickableOptions;
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
    pub disabled_checked_thumb_color: Color,
    pub disabled_checked_track_color: Color,
    pub disabled_unchecked_thumb_color: Color,
    pub disabled_unchecked_track_color: Color,
    pub disabled_unchecked_border_color: Color,
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
            // Opaque, over the surface, as Compose's `compositeOver(colorScheme.surface)`.
            disabled_checked_thumb_color: scheme.surface,
            disabled_checked_track_color: scheme
                .on_surface
                .with_alpha(0.12)
                .composite_over(scheme.surface),
            disabled_unchecked_thumb_color: scheme
                .on_surface
                .with_alpha(0.38)
                .composite_over(scheme.surface),
            disabled_unchecked_track_color: scheme
                .surface_container_highest
                .with_alpha(0.12)
                .composite_over(scheme.surface),
            disabled_unchecked_border_color: scheme
                .on_surface
                .with_alpha(0.12)
                .composite_over(scheme.surface),
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
        Some(on_checked_change) => minimum_interactive_component_size(
            modifier.toggleable_with(
                checked,
                ClickableOptions {
                    enabled,
                    role: Some(Role::Switch),
                    indication: if checked {
                        colors.checked_track_color
                    } else {
                        colors.unchecked_thumb_color
                    }
                    .into(),
                    ..ClickableOptions::DEFAULT
                },
                on_checked_change,
            ),
        ),
        None => modifier,
    };
    compose_foundation::layout::Spacer(
        modifier.required_size((52.dp(), 32.dp())).then_custom(SwitchElement {
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
    scope: compose_runtime::Owned<CoroutineScope>,
    /// A new value to animate to, started when drawn: a reset (reused for another item's switch,
    /// after the update giving it its value) shows the value at once instead (D435).
    animate: bool,
}

impl ModifierNodeElement for SwitchElement {
    type Node = SwitchNode;
    fn create(&self) -> SwitchNode {
        SwitchNode {
            checked: self.checked,
            enabled: self.enabled,
            colors: self.colors,
            progress: Animatable::new(if self.checked { 1.0 } else { 0.0 }),
            scope: CoroutineScope::new(),
            animate: false,
        }
    }
    fn update(&self, node: &mut SwitchNode) {
        node.animate |= node.checked != self.checked;
        node.checked = self.checked;
        node.enabled = self.enabled;
        node.colors = self.colors;
    }
    fn same(&self, other: &Self) -> bool {
        other.checked == self.checked
            && other.enabled == self.enabled
            && other.colors == self.colors
    }
    fn name(&self) -> &'static str {
        "switch"
    }
}

impl ModifierNode for SwitchNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_release(&mut self, _context: &mut compose_ui::ReleaseContext<'_>) {
        self.scope.cancel();
    }
    /// Reused for another item's switch: attached again after the pass that gave it its value
    /// (from the pass or from the kept pool), it shows the value at once, as a new one,
    /// without animating (D435, D1156).
    fn on_attach(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.animate = false;
        let target = if self.checked { 1.0 } else { 0.0 };
        if self.progress.value() != target || self.progress.is_running() {
            self.progress.snap_to(target);
        }
    }
}

pub(crate) fn blend(from: Color, to: Color, fraction: f32) -> Color {
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
        if std::mem::take(&mut self.animate) {
            let (progress, target) = (self.progress.clone(), if self.checked { 1.0 } else { 0.0 });
            self.scope.launch(async move {
                progress.animate_to(target, spring()).await;
            });
        }
        mirrored_in_rtl(scope, |scope| self.draw_start_to_end(scope));
    }
}

impl SwitchNode {
    /// Drawn from the start on the left (mirrored in a right-to-left layout).
    fn draw_start_to_end(&mut self, scope: &mut DrawScope<'_>) {
        // Read while drawing: each animation frame redraws the switch only.
        let progress = self.progress.value().clamp(0.0, 1.0);
        let size = scope.size();
        let density = scope.density();
        let colors = self.colors;
        // Material's disabled colors, as `SwitchDefaults.colors()`'s.
        let (checked_thumb, checked_track, unchecked_thumb, unchecked_track, unchecked_border) =
            if self.enabled {
                (
                    colors.checked_thumb_color,
                    colors.checked_track_color,
                    colors.unchecked_thumb_color,
                    colors.unchecked_track_color,
                    colors.unchecked_border_color,
                )
            } else {
                (
                    colors.disabled_checked_thumb_color,
                    colors.disabled_checked_track_color,
                    colors.disabled_unchecked_thumb_color,
                    colors.disabled_unchecked_track_color,
                    colors.disabled_unchecked_border_color,
                )
            };
        let track = Rect::new(0.0, 0.0, size.width, size.height);
        let radius = size.height / 2.0;
        scope.draw_round_rect(
            track,
            [radius; 4],
            Paint::fill(blend(unchecked_track, checked_track, progress)),
        );
        if progress < 1.0 {
            let border = density.to_px(2.dp());
            let half = border / 2.0;
            let inset = Rect::new(half, half, size.width - half, size.height - half);
            let opacity = f32::from(unchecked_border.alpha()) / 255.0 * (1.0 - progress);
            scope.draw_round_rect(
                inset,
                [radius - half; 4],
                Paint::stroke(unchecked_border.with_alpha(opacity), border),
            );
        }
        // The thumb: 16 dp unchecked, 24 dp checked, centered 16 dp from its end.
        let diameter = density.to_px(16.dp()) + density.to_px(8.dp()) * progress;
        let start = radius;
        let end = size.width - radius;
        let center = Offset::new(start + (end - start) * progress, size.height / 2.0);
        let half = diameter / 2.0;
        scope.draw_round_rect(
            Rect::new(center.x - half, center.y - half, center.x + half, center.y + half),
            [half; 4],
            Paint::fill(blend(unchecked_thumb, checked_thumb, progress)),
        );
    }
}

/// Draws with `draw`, mirrored around the node's center in a right-to-left layout: a switch's
/// thumb and a slider's track go from the right, as Compose's.
fn mirrored_in_rtl(scope: &mut DrawScope<'_>, draw: impl FnOnce(&mut DrawScope<'_>)) {
    if scope.layout_direction() != compose_ui::LayoutDirection::Rtl {
        draw(scope);
        return;
    }
    let size = scope.size();
    scope.save();
    scope.transform(Offset::new(size.width / 2.0, 0.0), (-1.0, 1.0), 0.0, Offset::ZERO);
    draw(scope);
    scope.restore();
}

// ---- RadioButton -----------------------------------------------------------------------------

/// A radio button's colors, as Compose's `RadioButtonColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RadioButtonColors {
    pub selected_color: Color,
    pub unselected_color: Color,
    pub disabled_selected_color: Color,
    pub disabled_unselected_color: Color,
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
            disabled_selected_color: scheme.on_surface.with_alpha(DISABLED_ALPHA),
            disabled_unselected_color: scheme.on_surface.with_alpha(DISABLED_ALPHA),
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
        Some(on_click) => {
            // A circle around the button, as Compose's `ripple(bounded = false, 20.dp)`.
            let options = ClickableOptions {
                enabled,
                role: Some(Role::RadioButton),
                indication: compose_ui::Indication::unbounded(color, 20.dp()),
                ..ClickableOptions::DEFAULT
            };
            minimum_interactive_component_size(modifier.clickable_with(options, on_click))
        }
        None => modifier,
    };
    let modifier = modifier
        .semantics(move |config| config.set_selected(selected))
        .padding(2.dp())
        .required_size((20.dp(), 20.dp()))
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
    /// The dot's radius in dp: 5 selected (Compose's 6 dp less half the 2 dp stroke), 0 not.
    dot: Animatable<f32>,
    scope: compose_runtime::Owned<CoroutineScope>,
    /// A new value to animate to, started when drawn (as a switch's).
    animate: bool,
}

impl ModifierNodeElement for RadioElement {
    type Node = RadioNode;
    fn create(&self) -> RadioNode {
        RadioNode {
            selected: self.selected,
            enabled: self.enabled,
            colors: self.colors,
            dot: Animatable::new(if self.selected { 5.0 } else { 0.0 }),
            scope: CoroutineScope::new(),
            animate: false,
        }
    }
    fn update(&self, node: &mut RadioNode) {
        node.animate |= node.selected != self.selected;
        node.selected = self.selected;
        node.enabled = self.enabled;
        node.colors = self.colors;
    }
    fn same(&self, other: &Self) -> bool {
        other.selected == self.selected
            && other.enabled == self.enabled
            && other.colors == self.colors
    }
    fn name(&self) -> &'static str {
        "radio_button"
    }
}

impl ModifierNode for RadioNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_release(&mut self, _context: &mut compose_ui::ReleaseContext<'_>) {
        self.scope.cancel();
    }
    /// Reused for another item's button: attached again, shown at its value, as a new one
    /// (D435, D1156).
    fn on_attach(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.animate = false;
        let target = if self.selected { 5.0 } else { 0.0 };
        if self.dot.value() != target || self.dot.is_running() {
            self.dot.snap_to(target);
        }
    }
}

impl DrawModifierNode for RadioNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        if std::mem::take(&mut self.animate) {
            let (dot, target) = (self.dot.clone(), if self.selected { 5.0 } else { 0.0 });
            // As Compose's `RadioAnimationDuration`.
            self.scope.launch(async move {
                dot.animate_to(target, tween(100)).await;
            });
        }
        let size = scope.size();
        let density = scope.density();
        let colors = &self.colors;
        let color = match (self.enabled, self.selected) {
            (true, true) => colors.selected_color,
            (true, false) => colors.unselected_color,
            (false, true) => colors.disabled_selected_color,
            (false, false) => colors.disabled_unselected_color,
        };
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
    /// The steps' ticks on the active and the inactive track.
    pub active_tick_color: Color,
    pub inactive_tick_color: Color,
    pub disabled_thumb_color: Color,
    pub disabled_active_track_color: Color,
    pub disabled_active_tick_color: Color,
    pub disabled_inactive_track_color: Color,
    pub disabled_inactive_tick_color: Color,
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
            active_tick_color: scheme.on_primary.with_alpha(0.38),
            inactive_tick_color: scheme.on_surface_variant.with_alpha(0.38),
            disabled_thumb_color: scheme
                .on_surface
                .with_alpha(DISABLED_ALPHA)
                .composite_over(scheme.surface),
            disabled_active_track_color: scheme.on_surface.with_alpha(DISABLED_ALPHA),
            disabled_active_tick_color: scheme.on_surface.with_alpha(DISABLED_ALPHA),
            disabled_inactive_track_color: scheme.on_surface.with_alpha(0.12),
            disabled_inactive_tick_color: scheme.on_surface.with_alpha(DISABLED_ALPHA),
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

/// Where `value` is in `range`, as a fraction.
fn fraction_in(range: (f32, f32), value: f32) -> f32 {
    let (start, end) = range;
    if end <= start { 0.0 } else { ((value - start) / (end - start)).clamp(0.0, 1.0) }
}

/// The value at `fraction` of `range`, snapped to the `steps`.
fn snapped(range: (f32, f32), steps: u32, fraction: f32) -> f32 {
    let mut fraction = fraction.clamp(0.0, 1.0);
    if steps > 0 {
        let intervals = (steps + 1) as f32;
        fraction = (fraction * intervals).round() / intervals;
    }
    let (start, end) = range;
    start + (end - start) * fraction
}

struct SliderNode {
    value: f32,
    on_value_change: Callback<dyn Fn(f32)>,
    on_value_change_finished: Option<Callback<dyn Fn()>>,
    enabled: bool,
    range: (f32, f32),
    steps: u32,
    colors: SliderColors,
    /// A press on the track, until it becomes a tap or a drag (or another handler takes it).
    press: Option<Offset>,
    dragging: bool,
    focused: bool,
    /// The thumb's radius in pixels, from the last measurement.
    radius: f32,
    /// The value last asked for, until the application gives the slider its new value: keys
    /// pressed before the next frame go on from it.
    asked: Option<f32>,
}

impl ModifierNodeElement for SliderElement {
    type Node = SliderNode;
    fn create(&self) -> SliderNode {
        SliderNode {
            value: self.value,
            on_value_change: self.on_value_change.clone(),
            on_value_change_finished: self.on_value_change_finished.clone(),
            enabled: self.enabled,
            range: self.range,
            steps: self.steps,
            colors: self.colors,
            press: None,
            dragging: false,
            focused: false,
            radius: 10.0,
            asked: None,
        }
    }
    fn update(&self, node: &mut SliderNode) {
        node.value = self.value;
        node.asked = None;
        node.on_value_change = self.on_value_change.clone();
        node.on_value_change_finished = self.on_value_change_finished.clone();
        node.enabled = self.enabled;
        node.range = self.range;
        node.steps = self.steps;
        node.colors = self.colors;
    }
    fn same(&self, other: &Self) -> bool {
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
    }
    fn name(&self) -> &'static str {
        "slider"
    }
}

/// The thumb's diameter, as Compose's (20 dp).
const THUMB: compose_ui::Dp = compose_ui::Dp(20.0);

impl SliderNode {
    fn fraction(&self) -> f32 {
        self.fraction_of(self.value)
    }

    fn fraction_of(&self, value: f32) -> f32 {
        fraction_in(self.range, value)
    }

    /// The value at `fraction` of the range, snapped to the steps.
    fn value_at(&self, fraction: f32) -> f32 {
        snapped(self.range, self.steps, fraction)
    }

    /// Asks the application for `value`: the slider shows the value it is given back (it is
    /// controlled), as Compose's.
    fn change_to(&mut self, value: f32, context: &mut EventContext<'_>) {
        if value != self.asked.unwrap_or(self.value) {
            self.asked = Some(value);
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
    fn on_reset(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.press = None;
        self.dragging = false;
        self.focused = false;
        self.asked = None;
    }
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
        mirrored_in_rtl(scope, |scope| self.draw_start_to_end(scope));
    }
}

impl SliderNode {
    /// Drawn from the start on the left (mirrored in a right-to-left layout).
    fn draw_start_to_end(&mut self, scope: &mut DrawScope<'_>) {
        let size = scope.size();
        let density = scope.density();
        let colors = &self.colors;
        let pick = |enabled: Color, disabled: Color| if self.enabled { enabled } else { disabled };
        let radius = density.to_px(THUMB) / 2.0;
        let track = density.to_px(4.dp());
        let center_y = size.height / 2.0;
        let (start, end) = (radius, (size.width - radius).max(radius));
        let x = start + (end - start) * self.fraction();
        let half = track / 2.0;
        scope.draw_round_rect(
            Rect::new(start, center_y - half, end, center_y + half),
            [half; 4],
            Paint::fill(pick(colors.inactive_track_color, colors.disabled_inactive_track_color)),
        );
        scope.draw_round_rect(
            Rect::new(start, center_y - half, x, center_y + half),
            [half; 4],
            Paint::fill(pick(colors.active_track_color, colors.disabled_active_track_color)),
        );
        // A tick at each stop, as Compose's.
        if self.steps > 0 {
            let tick = density.to_px(2.dp()) / 2.0;
            let stops = self.steps + 2;
            for stop in 0..stops {
                let at = start + (end - start) * stop as f32 / (stops - 1) as f32;
                let color = if at <= x {
                    pick(colors.active_tick_color, colors.disabled_active_tick_color)
                } else {
                    pick(colors.inactive_tick_color, colors.disabled_inactive_tick_color)
                };
                scope.draw_round_rect(
                    Rect::new(at - tick, center_y - tick, at + tick, center_y + tick),
                    [tick; 4],
                    Paint::fill(color),
                );
            }
        }
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
            Paint::fill(pick(colors.thumb_color, colors.disabled_thumb_color)),
        );
    }
}

impl PointerInputModifierNode for SliderNode {
    /// A tap moves the value there; a horizontal drag past the touch slop follows the pointer;
    /// a vertical one is left to a scrolling parent, as Compose's.
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
        // From the right in a right-to-left layout.
        let fraction = if context.layout_direction() == compose_ui::LayoutDirection::Rtl {
            1.0 - fraction
        } else {
            fraction
        };
        let slop = context.density().to_px(8.dp());
        match event.kind {
            PointerEventType::Press if !event.consumed => {
                self.press = Some(event.position);
                self.dragging = false;
            }
            PointerEventType::Move if self.dragging => {
                event.consume();
                let value = self.value_at(fraction);
                self.change_to(value, context);
            }
            PointerEventType::Move => {
                let Some(start) = self.press else { return };
                if event.consumed {
                    // A scrolling parent (or another handler) took it.
                    self.press = None;
                    return;
                }
                if (event.position.x - start.x).abs() > slop {
                    self.dragging = true;
                    event.consume();
                    context.request_focus();
                    context.invalidate_draw();
                    let value = self.value_at(fraction);
                    self.change_to(value, context);
                }
            }
            PointerEventType::Release => {
                let pressed = self.press.take();
                if std::mem::take(&mut self.dragging) {
                    event.consume();
                    context.invalidate_draw();
                    self.finish(context);
                } else if pressed.is_some() && !event.consumed {
                    event.consume();
                    context.request_focus();
                    let value = self.value_at(fraction);
                    self.change_to(value, context);
                    self.finish(context);
                }
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.press = None;
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
    /// The arrows move a step (or a hundredth), Home and End to the ends; the change finishes
    /// when the key is released, as Compose's `slideOnKeyEvents`.
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        if !self.enabled {
            return false;
        }
        let handled = matches!(
            event.key,
            Key::DirectionRight
                | Key::DirectionUp
                | Key::DirectionLeft
                | Key::DirectionDown
                | Key::MoveHome
                | Key::MoveEnd
        );
        if !handled {
            return false;
        }
        if event.kind == KeyEventType::KeyUp {
            self.finish(context);
            return true;
        }
        let step = if self.steps > 0 { 1.0 / (self.steps + 1) as f32 } else { 0.01 };
        let fraction = self.fraction_of(self.asked.unwrap_or(self.value));
        // Right is toward the start in a right-to-left layout, as Compose Desktop's.
        let rtl = context.layout_direction() == compose_ui::LayoutDirection::Rtl;
        let target = match event.key {
            Key::DirectionRight if rtl => fraction - step,
            Key::DirectionLeft if rtl => fraction + step,
            Key::DirectionRight | Key::DirectionUp => fraction + step,
            Key::DirectionLeft | Key::DirectionDown => fraction - step,
            Key::MoveHome => 0.0,
            Key::MoveEnd => 1.0,
            _ => return false,
        };
        let value = self.value_at(target);
        self.change_to(value, context);
        true
    }
}

impl SemanticsModifierNode for SliderNode {
    /// Its own node (as `progressSemantics`), its value in its range, and `SetProgress`.
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.is_merging_semantics_of_descendants = true;
        if self.enabled {
            config.set_focused(self.focused);
            config.request_focus_of_node();
        } else {
            config.disabled();
        }
        config.set_progress_bar_range_info(compose_ui::ProgressBarRangeInfo {
            current: self.value,
            range: self.range,
            steps: self.steps,
        });
        // Moves to the value asked (in the range, snapped to the steps), as Compose's slider's
        // `setProgress`.
        let (range, steps, enabled) = (self.range, self.steps, self.enabled);
        let current = self.asked.unwrap_or(self.value);
        let on_value_change = self.on_value_change.clone();
        let finished = self.on_value_change_finished.clone();
        config.set_progress(None, move |target| {
            if !enabled {
                return false;
            }
            let value = snapped(range, steps, fraction_in(range, target));
            if value == current {
                return false;
            }
            on_value_change(value);
            if let Some(finished) = &finished {
                finished();
            }
            true
        });
    }
}
