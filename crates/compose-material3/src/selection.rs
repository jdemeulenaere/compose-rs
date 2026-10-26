//! Selection controls, as Compose's: `Switch` and `RadioButton` (the slider is `slider.rs`).

use compose_animation::{Animatable, spring, tween};
use compose_runtime::{CoroutineScope, composable};
use compose_ui::ClickableOptions;
use compose_ui::{
    Color, DrawModifierNode, DrawScope, Modifier, ModifierNode, ModifierNodeElement, Offset, Paint,
    Rect, Role, UnitExt,
};
use std::cell::Cell;
use std::rc::Rc;

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
