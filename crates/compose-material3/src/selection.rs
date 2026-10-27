//! Selection controls, as Compose's: `Switch` and `RadioButton` (the slider is `slider.rs`).

use compose_animation::{Animatable, tween};
use compose_runtime::{CoroutineScope, composable};
use compose_ui::ClickableOptions;
use compose_ui::{
    Color, Dp, DrawModifierNode, DrawScope, Modifier, ModifierNode, ModifierNodeElement,
    MutableInteractionSource, Offset, Paint, Rect, Role, UnitExt,
};
use std::cell::Cell;
use std::rc::Rc;

use crate::components::minimum_interactive_component_size;
use crate::theme::MaterialTheme;

/// A disabled control's alpha, as Material's.
const DISABLED_ALPHA: f32 = 0.38;

// ---- Switch ----------------------------------------------------------------------------------

/// A switch's colors, as Compose's `SwitchColors`: its thumb's, track's, border's and the thumb
/// content's (icon), checked and unchecked, enabled and disabled.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SwitchColors {
    pub checked_thumb_color: Color,
    pub checked_track_color: Color,
    pub checked_border_color: Color,
    pub checked_icon_color: Color,
    pub unchecked_thumb_color: Color,
    pub unchecked_track_color: Color,
    pub unchecked_border_color: Color,
    pub unchecked_icon_color: Color,
    pub disabled_checked_thumb_color: Color,
    pub disabled_checked_track_color: Color,
    pub disabled_checked_border_color: Color,
    pub disabled_checked_icon_color: Color,
    pub disabled_unchecked_thumb_color: Color,
    pub disabled_unchecked_track_color: Color,
    pub disabled_unchecked_border_color: Color,
    pub disabled_unchecked_icon_color: Color,
}

impl compose_runtime::ComposeArg for SwitchColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl SwitchColors {
    /// The thumb's color, as `thumbColor(enabled, checked)`.
    pub fn thumb_color(&self, enabled: bool, checked: bool) -> Color {
        match (enabled, checked) {
            (true, true) => self.checked_thumb_color,
            (true, false) => self.unchecked_thumb_color,
            (false, true) => self.disabled_checked_thumb_color,
            (false, false) => self.disabled_unchecked_thumb_color,
        }
    }

    /// The track's color, as `trackColor(enabled, checked)`.
    pub fn track_color(&self, enabled: bool, checked: bool) -> Color {
        match (enabled, checked) {
            (true, true) => self.checked_track_color,
            (true, false) => self.unchecked_track_color,
            (false, true) => self.disabled_checked_track_color,
            (false, false) => self.disabled_unchecked_track_color,
        }
    }

    /// The track's border color, as `borderColor(enabled, checked)`.
    pub fn border_color(&self, enabled: bool, checked: bool) -> Color {
        match (enabled, checked) {
            (true, true) => self.checked_border_color,
            (true, false) => self.unchecked_border_color,
            (false, true) => self.disabled_checked_border_color,
            (false, false) => self.disabled_unchecked_border_color,
        }
    }

    /// The thumb content's color, as `iconColor(enabled, checked)`.
    pub fn icon_color(&self, enabled: bool, checked: bool) -> Color {
        match (enabled, checked) {
            (true, true) => self.checked_icon_color,
            (true, false) => self.unchecked_icon_color,
            (false, true) => self.disabled_checked_icon_color,
            (false, false) => self.disabled_unchecked_icon_color,
        }
    }
}

/// As Compose's `SwitchDefaults`.
pub struct SwitchDefaults;

impl SwitchDefaults {
    /// The thumb content's size, as `IconSize`.
    pub const ICON_SIZE: Dp = Dp(16.0);

    /// From the theme, as `SwitchDefaults.colors()` (`SwitchTokens`): the disabled colors
    /// opaque, over the surface.
    pub fn colors() -> SwitchColors {
        let scheme = MaterialTheme::color_scheme();
        let over =
            |color: Color, alpha: f32| color.with_alpha(alpha).composite_over(scheme.surface);
        SwitchColors {
            checked_thumb_color: scheme.on_primary,
            checked_track_color: scheme.primary,
            checked_border_color: Color::TRANSPARENT,
            checked_icon_color: scheme.on_primary_container,
            unchecked_thumb_color: scheme.outline,
            unchecked_track_color: scheme.surface_container_highest,
            unchecked_border_color: scheme.outline,
            unchecked_icon_color: scheme.surface_container_highest,
            disabled_checked_thumb_color: over(scheme.surface, 1.0),
            disabled_checked_track_color: over(scheme.on_surface, 0.12),
            disabled_checked_border_color: Color::TRANSPARENT,
            disabled_checked_icon_color: over(scheme.on_surface, 0.38),
            disabled_unchecked_thumb_color: over(scheme.on_surface, 0.38),
            disabled_unchecked_track_color: over(scheme.surface_container_highest, 0.12),
            disabled_unchecked_border_color: over(scheme.on_surface, 0.12),
            disabled_unchecked_icon_color: over(scheme.surface_container_highest, 0.38),
        }
    }
}

/// The track, as `SwitchTokens.TrackWidth` and `TrackHeight`, its outline `TrackOutlineWidth`.
const SWITCH_WIDTH: Dp = Dp(52.0);
const SWITCH_HEIGHT: Dp = Dp(32.0);
const TRACK_OUTLINE_WIDTH: Dp = Dp(2.0);
/// The thumb: checked (or with content) and unchecked, and pressed, as `ThumbDiameter`,
/// `UncheckedThumbDiameter` and `SwitchTokens.PressedHandleWidth`.
const THUMB_DIAMETER: Dp = Dp(24.0);
const UNCHECKED_THUMB_DIAMETER: Dp = Dp(16.0);
const PRESSED_THUMB_DIAMETER: Dp = Dp(28.0);
/// The thumb's state layer, as `SwitchTokens.StateLayerSize`.
const STATE_LAYER_SIZE: Dp = Dp(40.0);

/// A switch, as Compose's `Switch`: a 52 × 32 dp track (bordered, its color by the state) and
/// a thumb sliding to the end when checked, growing from 16 to 24 dp (24 dp with
/// `thumb_content`, which shows in it in the icon color) and to 28 dp while pressed, in a 48 dp
/// touch target, toggled by a click when `on_checked_change` is given.
#[composable]
pub fn Switch(
    checked: bool,
    on_checked_change: Option<impl Fn(bool) + 'static>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] thumb_content: Option<impl Fn() + 'static>,
    #[default(true)] enabled: bool,
    #[default(crate::SwitchDefaults::colors())] colors: SwitchColors,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    let modifier = match on_checked_change {
        // The toggle around the touch target, so all of it takes a press, as the checkbox's
        // (D1009).
        Some(on_checked_change) => minimum_interactive_component_size(modifier.toggleable_with(
            checked,
            ClickableOptions {
                enabled,
                role: Some(Role::Switch),
                indication: compose_ui::Indication::NONE,
                interaction_source: Some(interaction_source.clone()),
            },
            on_checked_change,
        )),
        None => modifier,
    };
    let track = compose_foundation::CIRCLE_SHAPE;
    let (thumb_color, icon_color) =
        (colors.thumb_color(enabled, checked), colors.icon_color(enabled, checked));
    // The thumb's state layer in the content color, as `ripple()`'s.
    let ripple_color = crate::theme::LOCAL_CONTENT_COLOR.current();
    let has_content = thumb_content.is_some();
    let modifier = modifier
        .wrap_content_size()
        .required_size((SWITCH_WIDTH, SWITCH_HEIGHT))
        .border_with(TRACK_OUTLINE_WIDTH, colors.border_color(enabled, checked), track)
        .background_with(colors.track_color(enabled, checked), track);
    compose_foundation::layout::Box(
        modifier,
        compose_ui::Alignment::TOP_START,
        false,
        compose_runtime::callback(move |scope: compose_ui::BoxScope| {
            let thumb_content = thumb_content.clone();
            let thumb = Modifier
                .align(scope, compose_ui::Alignment::CENTER_START)
                .then_custom(ThumbElement {
                    source: interaction_source.clone(),
                    checked,
                    has_content,
                })
                .then_custom(ThumbStateLayer {
                    source: interaction_source.clone(),
                    color: ripple_color,
                })
                .background_with(thumb_color, compose_foundation::CIRCLE_SHAPE);
            compose_foundation::layout::Box(
                thumb,
                compose_ui::Alignment::CENTER,
                false,
                compose_runtime::callback(move |_| {
                    if let Some(content) = thumb_content.clone() {
                        compose_runtime::CompositionLocalProvider(
                            crate::theme::LOCAL_CONTENT_COLOR.provides(icon_color),
                            &*content,
                        );
                    }
                }),
            );
        }),
    );
}

/// The thumb's size and place, as Compose's `ThumbElement`.
struct ThumbElement {
    source: MutableInteractionSource,
    checked: bool,
    has_content: bool,
}

/// As Compose's `ThumbNode`: the thumb's size and offset animated (the standard scheme's fast
/// spatial spring) to its state's, snapping while pressed.
struct ThumbNode {
    source: MutableInteractionSource,
    checked: bool,
    has_content: bool,
    size: Option<Animatable<f32>>,
    offset: Option<Animatable<f32>>,
    /// The first size and offset (pixels), which the animations start from.
    initial: Option<(f32, f32)>,
    scope: compose_runtime::Owned<CoroutineScope>,
}

impl ModifierNodeElement for ThumbElement {
    type Node = ThumbNode;
    fn create(&self) -> ThumbNode {
        ThumbNode {
            source: self.source.clone(),
            checked: self.checked,
            has_content: self.has_content,
            size: None,
            offset: None,
            initial: None,
            scope: CoroutineScope::new(),
        }
    }

    fn update(&self, node: &mut ThumbNode) {
        node.source = self.source.clone();
        node.checked = self.checked;
        node.has_content = self.has_content;
    }

    fn same(&self, other: &Self) -> bool {
        other.source == self.source
            && other.checked == self.checked
            && other.has_content == self.has_content
    }

    fn name(&self) -> &'static str {
        "switchThumb"
    }
}

impl ModifierNode for ThumbNode {
    fn as_layout(&mut self) -> Option<&mut dyn compose_ui::LayoutModifierNode> {
        Some(self)
    }

    fn on_release(&mut self, _context: &mut compose_ui::ReleaseContext<'_>) {
        self.scope.cancel();
    }

    /// Reused for another item's switch: at its value at once, as a new one (D435, D1156).
    fn on_reset(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.size = None;
        self.offset = None;
        self.initial = None;
    }
}

impl ThumbNode {
    /// The animated value going to `target`, on a spring (snapping while `pressed`).
    fn follow(
        &self,
        animatable: &Option<Animatable<f32>>,
        target: f32,
        pressed: bool,
    ) -> Option<f32> {
        let animatable = animatable.as_ref()?;
        if animatable.target_value() != target {
            let animatable = animatable.clone();
            let spec = if pressed {
                compose_animation::snap(0).into()
            } else {
                compose_animation::AnimationSpec::Spring(crate::motion::FAST_SPATIAL)
            };
            self.scope.launch(async move {
                animatable.animate_to(target, spec).await;
            });
        }
        Some(animatable.value())
    }
}

impl compose_ui::LayoutModifierNode for ThumbNode {
    fn measure(
        &mut self,
        scope: &mut compose_ui::MeasureScope<'_>,
        content: compose_ui::Measurable,
        _constraints: compose_ui::Constraints,
    ) -> compose_ui::MeasureResult {
        // Read while measuring: a press measures the thumb again.
        let pressed = self.source.is_pressed();
        let density = scope.density();
        let diameter = if pressed {
            PRESSED_THUMB_DIAMETER
        } else if self.has_content || self.checked {
            THUMB_DIAMETER
        } else {
            UNCHECKED_THUMB_DIAMETER
        };
        let size = density.to_px(diameter);
        let min_bound = density.to_px(Dp((SWITCH_HEIGHT.0 - diameter.0) / 2.0));
        let padding = (SWITCH_HEIGHT.0 - THUMB_DIAMETER.0) / 2.0;
        let max_bound = density.to_px(Dp(SWITCH_WIDTH.0 - THUMB_DIAMETER.0 - padding));
        let outline = density.to_px(TRACK_OUTLINE_WIDTH);
        let offset = match (pressed, self.checked) {
            (true, true) => max_bound - outline,
            (true, false) => outline,
            (false, true) => max_bound,
            (false, false) => min_bound,
        };
        match self.initial {
            None => self.initial = Some((size, offset)),
            Some((initial_size, initial_offset)) => {
                self.size.get_or_insert_with(|| Animatable::new(initial_size));
                self.offset.get_or_insert_with(|| Animatable::new(initial_offset));
            }
        }
        // Read while measuring: each frame of the animations measures the thumb again.
        let actual = self.follow(&self.size, size, pressed).unwrap_or(size) as i32;
        let x = self.follow(&self.offset, offset, pressed).unwrap_or(offset) as i32;
        let placeable = scope.measure(content, compose_ui::Constraints::fixed(actual, actual));
        scope.place_relative(&placeable, x, 0);
        scope.layout(actual, actual)
    }
}

/// The thumb's state layer: a `STATE_LAYER_SIZE` circle around it while its switch is pressed,
/// focused or hovered, as Compose's `indication(interactionSource, ripple(bounded = false))`.
struct ThumbStateLayer {
    source: MutableInteractionSource,
    color: Color,
}

impl ModifierNodeElement for ThumbStateLayer {
    type Node = ThumbStateLayer;
    fn create(&self) -> ThumbStateLayer {
        ThumbStateLayer { source: self.source.clone(), color: self.color }
    }

    fn update(&self, node: &mut ThumbStateLayer) {
        node.source = self.source.clone();
        node.color = self.color;
    }

    fn same(&self, other: &Self) -> bool {
        other.source == self.source && other.color == self.color
    }

    fn name(&self) -> &'static str {
        "switchThumbIndication"
    }
}

impl ModifierNode for ThumbStateLayer {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for ThumbStateLayer {
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        // Read while drawing: a press, focus or hover draws the thumb again.
        let alpha = if self.source.is_pressed() || self.source.is_focused() {
            0.10
        } else if self.source.is_hovered() {
            0.08
        } else {
            return;
        };
        let size = scope.size();
        let center = Offset::new(size.width / 2.0, size.height / 2.0);
        let radius = scope.density().to_px(STATE_LAYER_SIZE) / 2.0;
        scope.draw_round_rect(
            Rect::new(center.x - radius, center.y - radius, center.x + radius, center.y + radius),
            [radius; 4],
            Paint::fill(self.color.with_alpha(alpha)),
        );
    }
}

/// `from` blended toward `to` by `fraction`, channel by channel.
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
