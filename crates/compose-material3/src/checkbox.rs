//! `Checkbox` and `TriStateCheckbox`, as Compose's Material 3 at androidx-main's tip: a 20 dp
//! box (2 dp corners) in a 48 dp touch target while clickable, its colors animating with the
//! standard motion scheme's effects springs, its check mark drawn in as it's checked and moved
//! to a line when indeterminate.

use compose_runtime::composable;
use compose_ui::{
    ClickableOptions, Color, DrawScope, Modifier, MutableInteractionSource, Offset, Rect, Role,
    Stroke, StrokeCap, ToggleableState, UnitExt,
};

use crate::Dp;
use crate::components::minimum_interactive_component_size;
use crate::theme::MaterialTheme;

/// A checkbox's colors, as Compose's `CheckboxColors`: its check mark's, its box's and its
/// border's, checked, unchecked and indeterminate, enabled and disabled. Change some with struct
/// update syntax over [`CheckboxDefaults::colors`] (D180).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CheckboxColors {
    pub checked_checkmark_color: Color,
    pub unchecked_checkmark_color: Color,
    pub disabled_checkmark_color: Color,
    pub checked_box_color: Color,
    pub unchecked_box_color: Color,
    pub disabled_checked_box_color: Color,
    pub disabled_unchecked_box_color: Color,
    pub disabled_indeterminate_box_color: Color,
    pub checked_border_color: Color,
    pub unchecked_border_color: Color,
    pub disabled_border_color: Color,
    pub disabled_unchecked_border_color: Color,
    pub disabled_indeterminate_border_color: Color,
}

impl compose_runtime::ComposeArg for CheckboxColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl CheckboxColors {
    /// The check mark's color in `state`, as `checkmarkColor(state)` (Compose's, while its
    /// `isCheckboxStylingFixEnabled` flag is off: the same enabled or not).
    pub fn checkmark_color(&self, state: ToggleableState) -> Color {
        if state == ToggleableState::Off {
            self.unchecked_checkmark_color
        } else {
            self.checked_checkmark_color
        }
    }

    /// The box's color, as `boxColor(enabled, state)`.
    pub fn box_color(&self, enabled: bool, state: ToggleableState) -> Color {
        match (enabled, state) {
            (true, ToggleableState::On | ToggleableState::Indeterminate) => self.checked_box_color,
            (true, ToggleableState::Off) => self.unchecked_box_color,
            (false, ToggleableState::On) => self.disabled_checked_box_color,
            (false, ToggleableState::Indeterminate) => self.disabled_indeterminate_box_color,
            (false, ToggleableState::Off) => self.disabled_unchecked_box_color,
        }
    }

    /// The border's color, as `borderColor(enabled, state)`.
    pub fn border_color(&self, enabled: bool, state: ToggleableState) -> Color {
        match (enabled, state) {
            (true, ToggleableState::On | ToggleableState::Indeterminate) => {
                self.checked_border_color
            }
            (true, ToggleableState::Off) => self.unchecked_border_color,
            (false, ToggleableState::Indeterminate) => self.disabled_indeterminate_border_color,
            (false, ToggleableState::On) => self.disabled_border_color,
            (false, ToggleableState::Off) => self.disabled_unchecked_border_color,
        }
    }
}

/// As Compose's `CheckboxDefaults`.
pub struct CheckboxDefaults {}

impl CheckboxDefaults {
    /// The width of the box's border and of the check mark, as `StrokeWidth`.
    pub const STROKE_WIDTH: Dp = Dp(2.0);

    /// From the theme, as `CheckboxDefaults.colors()` (`CheckboxTokens`): the box and its
    /// border in the primary color checked, the check mark on it; the border alone in
    /// on-surface-variant unchecked; disabled, on-surface at 38 %, the check mark in the
    /// surface color.
    pub fn colors() -> CheckboxColors {
        let scheme = MaterialTheme::color_scheme();
        let disabled = scheme.on_surface.with_alpha(0.38);
        CheckboxColors {
            checked_checkmark_color: scheme.on_primary,
            unchecked_checkmark_color: Color::TRANSPARENT,
            disabled_checkmark_color: scheme.surface,
            checked_box_color: scheme.primary,
            unchecked_box_color: Color::TRANSPARENT,
            disabled_checked_box_color: disabled,
            disabled_unchecked_box_color: Color::TRANSPARENT,
            disabled_indeterminate_box_color: disabled,
            checked_border_color: scheme.primary,
            unchecked_border_color: scheme.on_surface_variant,
            disabled_border_color: disabled,
            disabled_unchecked_border_color: disabled,
            disabled_indeterminate_border_color: disabled,
        }
    }

    /// The colors from one color per state, as the `colors(checkedColor, uncheckedColor,
    /// checkmarkColor, disabledCheckedColor, disabledUncheckedColor, disabledIndeterminateColor)`
    /// overload: the checked color fills the box and draws its border, the unchecked one the
    /// border alone (the box transparent); `Color::UNSPECIFIED` keeps the default's.
    pub fn colors_with(
        checked_color: Color,
        unchecked_color: Color,
        checkmark_color: Color,
        disabled_checked_color: Color,
        disabled_unchecked_color: Color,
        disabled_indeterminate_color: Color,
    ) -> CheckboxColors {
        let default = CheckboxDefaults::colors();
        CheckboxColors {
            checked_checkmark_color: checkmark_color.or(default.checked_checkmark_color),
            unchecked_checkmark_color: Color::TRANSPARENT,
            disabled_checkmark_color: checkmark_color.or(default.disabled_checkmark_color),
            checked_box_color: checked_color.or(default.checked_box_color),
            unchecked_box_color: Color::TRANSPARENT,
            disabled_checked_box_color: disabled_checked_color
                .or(default.disabled_checked_box_color),
            disabled_unchecked_box_color: Color::TRANSPARENT,
            disabled_indeterminate_box_color: disabled_indeterminate_color
                .or(default.disabled_indeterminate_box_color),
            checked_border_color: checked_color.or(default.checked_border_color),
            unchecked_border_color: unchecked_color.or(default.unchecked_border_color),
            disabled_border_color: disabled_checked_color.or(default.disabled_border_color),
            disabled_unchecked_border_color: disabled_unchecked_color
                .or(default.disabled_unchecked_border_color),
            disabled_indeterminate_border_color: disabled_indeterminate_color
                .or(default.disabled_indeterminate_border_color),
        }
    }
}

/// A checkbox, as Compose's `Checkbox`: a 20 dp box in a 48 dp touch target, toggled by a
/// click when `on_checked_change` is given (none, and nothing to click: it's part of what's
/// around it). `checkmark_stroke` and `outline_stroke` are Compose's stroke overload's (`None`:
/// `CheckboxDefaults::STROKE_WIDTH`, the check mark's ends square).
#[composable]
#[allow(clippy::too_many_arguments)]
pub fn Checkbox(
    checked: bool,
    on_checked_change: Option<impl Fn(bool) + 'static>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::CheckboxDefaults::colors())] colors: CheckboxColors,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    #[default(None)] checkmark_stroke: Option<Stroke>,
    #[default(None)] outline_stroke: Option<Stroke>,
) {
    let on_click = on_checked_change
        .map(|on_checked_change| compose_runtime::callback(move || on_checked_change(!checked)));
    checkbox(
        ToggleableState::from(checked),
        on_click,
        modifier,
        enabled,
        colors,
        interaction_source,
        checkmark_stroke,
        outline_stroke,
    );
}

/// A checkbox of three states, as Compose's `TriStateCheckbox`: on, off or indeterminate (a
/// line), `on_click` deciding the next (a parent of checkboxes, say).
#[composable]
#[allow(clippy::too_many_arguments)]
pub fn TriStateCheckbox(
    state: ToggleableState,
    on_click: Option<impl Fn() + 'static>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::CheckboxDefaults::colors())] colors: CheckboxColors,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    #[default(None)] checkmark_stroke: Option<Stroke>,
    #[default(None)] outline_stroke: Option<Stroke>,
) {
    checkbox(
        state,
        on_click,
        modifier,
        enabled,
        colors,
        interaction_source,
        checkmark_stroke,
        outline_stroke,
    );
}

/// Before an unchecking's check mark and box go, as `SnapAnimationDelay`.
const SNAP_ANIMATION_DELAY: u32 = 100;
/// The box, as `CheckboxSize`, with `CheckboxDefaultPadding` around it and `RadiusSize` corners.
const CHECKBOX_SIZE: Dp = Dp(20.0);
const CHECKBOX_PADDING: Dp = Dp(2.0);
const RADIUS_SIZE: Dp = Dp(2.0);

/// As Compose's `CheckboxImpl`.
#[allow(clippy::too_many_arguments)]
fn checkbox(
    state: ToggleableState,
    on_click: Option<compose_runtime::Callback<dyn Fn()>>,
    modifier: Modifier,
    enabled: bool,
    colors: CheckboxColors,
    interaction_source: Option<MutableInteractionSource>,
    checkmark_stroke: Option<Stroke>,
    outline_stroke: Option<Stroke>,
) {
    let source = crate::components::remembered_interaction_source(interaction_source);
    let modifier = match on_click {
        Some(on_click) => {
            // A circle around the box, as Compose's `ripple(bounded = false, 20.dp)`, in the
            // content color (`isCheckboxStylingFixEnabled` off, its default).
            let indication = crate::ripple::ripple_with(crate::ripple::RippleOptions {
                bounded: false,
                radius: 20.dp(),
                focus_ring_shape: Some(compose_ui::Shape::Rounded(
                    [compose_ui::CornerSize::Percent(25.0); 4],
                )),
                ..crate::ripple::RippleOptions::default()
            });
            let options = ClickableOptions {
                enabled,
                role: Some(Role::Checkbox),
                indication: compose_ui::Indication::NONE,
                interaction_source: Some(source.clone()),
                ..ClickableOptions::DEFAULT
            };
            // The toggle around the touch target, so all 48 dp take a press: compose-ui hit
            // tests a node's bounds only, where Compose's toggle (inside the touch target)
            // takes the target's presses by extending its hit test (D1009). Its ripple inside,
            // on the box, as Compose's: what a focus ring goes around.
            minimum_interactive_component_size(
                modifier.tri_state_toggleable_with(state, options, on_click),
            )
            .indication(source, indication)
        }
        None => modifier,
    };
    let modifier = modifier
        .padding(CHECKBOX_PADDING)
        .wrap_content_size()
        .required_size((CHECKBOX_SIZE, CHECKBOX_SIZE));

    // The check mark draws in from an unchecked box; it goes 100 ms after an unchecking; it
    // gravitates to a line for the indeterminate state, as Compose's transition.
    let transition = compose_animation::update_transition(state, "Checkbox");
    let motion = MaterialTheme::motion_scheme();
    let spatial = motion.default_spatial_spec();
    let check_fraction = transition.animate_float(
        {
            let spatial = spatial.clone();
            move |segment| match (&segment.initial_state, &segment.target_state) {
                (_, ToggleableState::Off) => compose_animation::snap(SNAP_ANIMATION_DELAY).into(),
                _ => spatial.clone(),
            }
        },
        |state| if *state == ToggleableState::Off { 0.0 } else { 1.0 },
    );
    let gravitation = transition.animate_float(
        move |segment| match (&segment.initial_state, &segment.target_state) {
            (ToggleableState::Off, _) => compose_animation::snap(0).into(),
            (_, ToggleableState::Off) => compose_animation::snap(SNAP_ANIMATION_DELAY).into(),
            _ => spatial.clone(),
        },
        |state| if *state == ToggleableState::Indeterminate { 1.0 } else { 0.0 },
    );
    // The box comes in with the default effects spec, goes with the fast one.
    let color_spec = if state == ToggleableState::Off {
        motion.fast_effects_spec()
    } else {
        motion.default_effects_spec()
    };
    let animate = |target: Color, label| {
        compose_animation::animate_color_as_state(target, color_spec.clone(), label, None)
    };
    let checkmark_color = animate(colors.checkmark_color(state), "CheckmarkColor");
    // Disabled, the colors snap: there's no animating between enabled and disabled.
    let box_color = enabled.then(|| animate(colors.box_color(enabled, state), "BoxColor"));
    let border_color = enabled.then(|| animate(colors.border_color(enabled, state), "BorderColor"));
    let (disabled_box, disabled_border) =
        (colors.box_color(enabled, state), colors.border_color(enabled, state));
    compose_foundation::layout::Canvas(modifier, move |scope| {
        let default_width = (scope.density().to_px(CheckboxDefaults::STROKE_WIDTH)).floor();
        let outline = outline_stroke.unwrap_or(Stroke::new(default_width));
        let checkmark = checkmark_stroke
            .unwrap_or(Stroke { cap: StrokeCap::Square, ..Stroke::new(default_width) });
        let radius = scope.density().to_px(RADIUS_SIZE);
        draw_box(
            scope,
            box_color.map_or(disabled_box, |state| state.get()),
            border_color.map_or(disabled_border, |state| state.get()),
            radius,
            outline,
        );
        draw_check(
            scope,
            checkmark_color.get(),
            check_fraction.get(),
            gravitation.get(),
            checkmark,
        );
    });
}

/// The box: filled when its border is its color, else filled inside the border and the border
/// stroked, as Compose's `drawBox`.
fn draw_box(scope: &mut DrawScope<'_>, fill: Color, border: Color, radius: f32, stroke: Stroke) {
    let size = scope.size().width;
    let width = stroke.width;
    if fill == border {
        scope.draw_round_rect(
            Rect::new(0.0, 0.0, size, size),
            [radius; 4],
            compose_ui::Paint::fill(fill),
        );
    } else {
        let inner = (radius - width).max(0.0);
        scope.draw_round_rect(
            Rect::new(width, width, size - width, size - width),
            [inner; 4],
            compose_ui::Paint::fill(fill),
        );
        let half = width / 2.0;
        scope.draw_round_rect(
            Rect::new(half, half, size - half, size - half),
            [radius - half; 4],
            stroke.paint(border),
        );
    }
}

/// The check mark, its first `fraction` along its two strokes, gravitating to a centered line
/// by `gravitation`, as Compose's `drawCheck` (its `PathMeasure.getSegment` of a two-line
/// path, measured here). Drawn as its two lines: their square ends meet at the check's right
/// angle as the path's miter join does (and nearly so on the way to the line), without a path
/// to record every frame of the animation.
fn draw_check(
    scope: &mut DrawScope<'_>,
    color: Color,
    fraction: f32,
    gravitation: f32,
    stroke: Stroke,
) {
    if fraction <= 0.0 {
        return;
    }
    let width = scope.size().width;
    let lerp = |from: f32, to: f32| from + (to - from) * gravitation;
    let points = [
        (0.2 * width, lerp(0.5, 0.5) * width),
        (lerp(0.4, 0.5) * width, lerp(0.7, 0.5) * width),
        (0.8 * width, lerp(0.3, 0.5) * width),
    ];
    let length = |(ax, ay): (f32, f32), (bx, by): (f32, f32)| (bx - ax).hypot(by - ay);
    let (first, second) = (length(points[0], points[1]), length(points[1], points[2]));
    let mut left = (first + second) * fraction.min(1.0);
    let paint = stroke.paint(color);
    for (segment, (from, to)) in [(first, (points[0], points[1])), (second, (points[1], points[2]))]
    {
        if left <= 0.0 {
            break;
        }
        let part = if segment > 0.0 { (left / segment).min(1.0) } else { 1.0 };
        let end = Offset::new(from.0 + (to.0 - from.0) * part, from.1 + (to.1 - from.1) * part);
        scope.draw_line(Offset::new(from.0, from.1), end, paint);
        left -= segment;
    }
}
