//! Segmented buttons, as Compose's Material 3: `SingleChoiceSegmentedButtonRow` and its
//! `SegmentedButton`s, with `SegmentedButtonDefaults` and `SegmentedButtonColors`.

use std::rc::Rc;

use compose_foundation::BorderStroke;
use compose_foundation::layout::Box;
use compose_runtime::{CompositionLocalProvider, composable, remember};
use compose_ui::{
    Alignment, Color, Constraints, CornerSize, Dp, Layout, Measurable, MeasurePolicy,
    MeasureResult, MeasureScope, Modifier, MutableInteractionSource, Role, Shape, UnitExt,
};

use crate::components::{SurfaceClick, remembered_interaction_source, surface};
use crate::theme::{LOCAL_TEXT_STYLE, MaterialTheme};

/// The scope of a `SingleChoiceSegmentedButtonRow`'s content, as Compose's
/// `SingleChoiceSegmentedButtonRowScope`: what its `SegmentedButton`s take.
#[derive(Clone, Copy, Debug)]
pub struct SingleChoiceSegmentedButtonRowScope(());

impl compose_runtime::ComposeArg for SingleChoiceSegmentedButtonRowScope {
    fn same(&self, _: &Self) -> bool {
        true
    }
}

/// A segmented button's colors, as Compose's `SegmentedButtonColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SegmentedButtonColors {
    pub active_container_color: Color,
    pub active_content_color: Color,
    pub active_border_color: Color,
    pub inactive_container_color: Color,
    pub inactive_content_color: Color,
    pub inactive_border_color: Color,
    pub disabled_active_container_color: Color,
    pub disabled_active_content_color: Color,
    pub disabled_active_border_color: Color,
    pub disabled_inactive_container_color: Color,
    pub disabled_inactive_content_color: Color,
    pub disabled_inactive_border_color: Color,
}

impl SegmentedButtonColors {
    /// As `containerColor(enabled, active)`.
    pub fn container_color(&self, enabled: bool, active: bool) -> Color {
        match (enabled, active) {
            (true, true) => self.active_container_color,
            (true, false) => self.inactive_container_color,
            (false, true) => self.disabled_active_container_color,
            (false, false) => self.disabled_inactive_container_color,
        }
    }

    /// As `contentColor(enabled, checked)`.
    pub fn content_color(&self, enabled: bool, active: bool) -> Color {
        match (enabled, active) {
            (true, true) => self.active_content_color,
            (true, false) => self.inactive_content_color,
            (false, true) => self.disabled_active_content_color,
            (false, false) => self.disabled_inactive_content_color,
        }
    }

    /// As `borderColor(enabled, active)`.
    pub fn border_color(&self, enabled: bool, active: bool) -> Color {
        match (enabled, active) {
            (true, true) => self.active_border_color,
            (true, false) => self.inactive_border_color,
            (false, true) => self.disabled_active_border_color,
            (false, false) => self.disabled_inactive_border_color,
        }
    }
}

impl compose_runtime::ComposeArg for SegmentedButtonColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `SegmentedButtonDefaults`.
pub struct SegmentedButtonDefaults;

impl SegmentedButtonDefaults {
    /// The outline's width, and how much neighbors overlap, as `BorderWidth`.
    pub const BORDER_WIDTH: Dp = Dp(1.0);
    /// The check's size, as `IconSize`.
    pub const ICON_SIZE: Dp = Dp(18.0);

    /// As `colors()`: the secondary container when active, outlined.
    pub fn colors() -> SegmentedButtonColors {
        let scheme = MaterialTheme::color_scheme();
        let disabled_content = scheme.on_surface.with_alpha(0.38);
        let disabled_border = scheme.on_surface.with_alpha(0.12);
        SegmentedButtonColors {
            active_container_color: scheme.secondary_container,
            active_content_color: scheme.on_secondary_container,
            active_border_color: scheme.outline,
            inactive_container_color: scheme.surface,
            inactive_content_color: scheme.on_surface,
            inactive_border_color: scheme.outline,
            disabled_active_container_color: scheme.secondary_container,
            disabled_active_content_color: disabled_content,
            disabled_active_border_color: disabled_border,
            disabled_inactive_container_color: scheme.surface,
            disabled_inactive_content_color: disabled_content,
            disabled_inactive_border_color: disabled_border,
        }
    }

    /// A pill, as `baseShape`.
    pub fn base_shape() -> Shape {
        compose_foundation::CIRCLE_SHAPE
    }

    /// The shape of the button at `index` of `count`: the base shape's start corners for the
    /// first, its end ones for the last, square between, as `itemShape(index, count)`.
    pub fn item_shape(index: usize, count: usize) -> Shape {
        let base = SegmentedButtonDefaults::base_shape();
        if count <= 1 {
            return base;
        }
        let Shape::Rounded([top_start, top_end, bottom_end, bottom_start]) = base else {
            return base;
        };
        let none = CornerSize::Dp(Dp(0.0));
        if index == 0 {
            Shape::Rounded([top_start, none, none, bottom_start])
        } else if index == count - 1 {
            Shape::Rounded([none, top_end, bottom_end, none])
        } else {
            compose_ui::RECTANGLE_SHAPE
        }
    }

    /// An outline of `color`, as `borderStroke(color)`.
    pub fn border_stroke(color: Color) -> BorderStroke {
        BorderStroke::new(SegmentedButtonDefaults::BORDER_WIDTH, color)
    }

    /// A check mark when `active`, as `Icon(active)` (shown at once, where Compose animates
    /// it in).
    pub fn icon(active: bool) {
        if active {
            crate::icon::Icon(
                crate::icons::filled::check(),
                None,
                Modifier.size(SegmentedButtonDefaults::ICON_SIZE),
                crate::theme::LOCAL_CONTENT_COLOR.current(),
            );
        }
    }
}

/// A row of segmented buttons of which one is selected, as Compose's
/// `SingleChoiceSegmentedButtonRow`: at least 40 dp high, every button as wide as the widest
/// (sharing the row's width when it's given one), neighbors overlapping by `space` so their
/// outlines meet.
#[composable]
pub fn SingleChoiceSegmentedButtonRow(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::SegmentedButtonDefaults::BORDER_WIDTH)] space: Dp,
    content: impl Fn(SingleChoiceSegmentedButtonRowScope) + 'static,
) {
    let policy = compose_runtime::remember_keyed(space, move || {
        Rc::new(SegmentedRowLayout { space }) as Rc<dyn MeasurePolicy>
    });
    Layout(
        modifier.selectable_group(),
        policy,
        compose_runtime::callback(move || content(SingleChoiceSegmentedButtonRowScope(()))),
    );
}

/// The buttons, each as wide as the widest (or a share of the minimum width), as tall as the
/// tallest and at least 40 dp, overlapping by `space`.
struct SegmentedRowLayout {
    space: Dp,
}

impl MeasurePolicy for SegmentedRowLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let count = measurables.len() as i32;
        if count == 0 {
            return scope.layout(constraints.min_width, constraints.min_height);
        }
        let density = scope.density();
        let space = density.round_to_px(self.space);
        let widest = measurables
            .iter()
            .map(|&button| scope.max_intrinsic_width(button, Constraints::INFINITY))
            .max()
            .unwrap_or(0);
        // Their total less the overlaps: the widest each, or the minimum (the maximum) shared,
        // the extra pixels to the first ones.
        let overlaps = space * (count - 1);
        let mut total = widest * count;
        if total - overlaps < constraints.min_width {
            total = constraints.min_width + overlaps;
        }
        if constraints.max_width != Constraints::INFINITY
            && total - overlaps > constraints.max_width
        {
            total = constraints.max_width + overlaps;
        }
        let width_of = |index: i32| total / count + i32::from(index < total % count);
        let minimum = density.round_to_px(40.dp()).max(constraints.min_height);
        let height = measurables
            .iter()
            .map(|&button| scope.max_intrinsic_height(button, total / count))
            .max()
            .unwrap_or(0)
            .max(minimum)
            .min(constraints.max_height);
        let mut x = 0;
        for (index, &button) in measurables.iter().enumerate() {
            let width = width_of(index as i32);
            let button = scope.measure(button, Constraints::new(width, width, 0, height));
            scope.place_relative(&button, x, (height - button.height) / 2);
            x += width - space;
        }
        scope.layout(constraints.constrain_width(total - overlaps), height)
    }

    fn debug_name(&self) -> &'static str {
        "SingleChoiceSegmentedButtonRow"
    }
}

/// A button of a `SingleChoiceSegmentedButtonRow`, as Compose's `SegmentedButton(selected,
/// onClick, shape, …)`: the shape `SegmentedButtonDefaults::item_shape` gives its place, in the
/// active colors when `selected`, its icon (by default a check when selected) and label
/// (label-large) centered, 12 dp from the sides; a radio button for accessibility.
/// `border` `None` is the default outline in `colors`' border color; `icon` `None` the default
/// check (`Some(|| {})` for none).
#[composable]
pub fn SegmentedButton(
    scope: SingleChoiceSegmentedButtonRowScope,
    selected: bool,
    on_click: impl Fn() + 'static,
    shape: Shape,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::SegmentedButtonDefaults::colors())] colors: SegmentedButtonColors,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    label: impl Fn() + 'static,
) {
    let _ = scope;
    let border = border.unwrap_or_else(|| {
        SegmentedButtonDefaults::border_stroke(colors.border_color(enabled, selected))
    });
    let interaction_source = remembered_interaction_source(interaction_source);
    let mut click = Some(SurfaceClick {
        on_click,
        enabled,
        role: None,
        interaction_source,
        selected: Some(selected),
        checked: None,
        elevations: None,
    });
    let label_style = MaterialTheme::typography().label_large.clone();
    let policy = remember(|| Rc::new(IconLabelLayout) as Rc<dyn MeasurePolicy>);
    let modifier = modifier
        .size_in(compose_ui::SizeSpec::bounds(
            crate::ButtonDefaults::MIN_WIDTH,
            crate::ButtonDefaults::MIN_HEIGHT,
            Dp::UNSPECIFIED,
            Dp::UNSPECIFIED,
        ))
        .semantics(|config| config.set_role(Role::RadioButton));
    let content = compose_runtime::callback(move || {
        let (icon, label) = (icon.clone(), label.clone());
        Layout(
            Modifier.padding(crate::ButtonDefaults::TEXT_BUTTON_CONTENT_PADDING),
            policy.clone(),
            compose_runtime::callback(move || {
                let icon = icon.clone();
                Box(
                    Modifier,
                    Alignment::CENTER,
                    false,
                    compose_runtime::callback(move |_| match &icon {
                        Some(icon) => icon(),
                        None => SegmentedButtonDefaults::icon(selected),
                    }),
                );
                let label = label.clone();
                Box(
                    Modifier,
                    Alignment::CENTER,
                    false,
                    compose_runtime::callback(move |_| label()),
                );
            }),
        );
    });
    CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(label_style), || {
        surface(
            modifier.clone(),
            shape,
            colors.container_color(enabled, selected),
            colors.content_color(enabled, selected),
            Dp(0.0),
            Dp(0.0),
            Some(border),
            click.take(),
            content.clone(),
        );
    });
}

/// The icon, then (if the icon shows) 8 dp, then the label, centered in the button, as
/// Compose's `SegmentedButtonContent`.
struct IconLabelLayout;

impl MeasurePolicy for IconLabelLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let [icon, label] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let loose = constraints.loosen();
        let icon = scope.measure(*icon, loose);
        let spacing = if icon.width > 0 {
            scope.density().round_to_px(crate::ButtonDefaults::ICON_SPACING)
        } else {
            0
        };
        let label = scope.measure(*label, loose);
        // Centered in the button (its minimum size).
        let content_width = icon.width + spacing + label.width;
        let width = constraints.constrain_width(content_width);
        let height = constraints.constrain_height(icon.height.max(label.height));
        let start = (width - content_width) / 2;
        scope.place_relative(&icon, start, (height - icon.height) / 2);
        scope.place_relative(&label, start + icon.width + spacing, (height - label.height) / 2);
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "SegmentedButtonContent"
    }
}
