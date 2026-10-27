//! Segmented buttons, as Compose's Material 3: `SingleChoiceSegmentedButtonRow` and its
//! `SegmentedButton`s, `MultiChoiceSegmentedButtonRow` and its `CheckableSegmentedButton`s, with
//! `SegmentedButtonDefaults` and `SegmentedButtonColors`.

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

/// The scope of a `MultiChoiceSegmentedButtonRow`'s content, as Compose's
/// `MultiChoiceSegmentedButtonRowScope`: what its `CheckableSegmentedButton`s take.
#[derive(Clone, Copy, Debug)]
pub struct MultiChoiceSegmentedButtonRowScope(());

impl compose_runtime::ComposeArg for MultiChoiceSegmentedButtonRowScope {
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
    /// Around a button's content, as `ContentPadding`: 12 dp at the sides, the buttons' 8 dp
    /// above and below.
    pub const CONTENT_PADDING: compose_ui::PaddingValues =
        compose_ui::PaddingValues::new(Dp(12.0), Dp(8.0), Dp(12.0), Dp(8.0));

    /// As `colors()` (`OutlinedSegmentedButtonTokens`): the secondary container when active,
    /// no container inactive, outlined (the outline at 12 % disabled).
    pub fn colors() -> SegmentedButtonColors {
        let scheme = MaterialTheme::color_scheme();
        let disabled_content = scheme.on_surface.with_alpha(0.38);
        let disabled_border = scheme.outline.with_alpha(0.12);
        SegmentedButtonColors {
            active_container_color: scheme.secondary_container,
            active_content_color: scheme.on_secondary_container,
            active_border_color: scheme.outline,
            inactive_container_color: Color::TRANSPARENT,
            inactive_content_color: scheme.on_surface,
            inactive_border_color: scheme.outline,
            disabled_active_container_color: scheme.secondary_container,
            disabled_active_content_color: disabled_content,
            disabled_active_border_color: disabled_border,
            disabled_inactive_container_color: Color::TRANSPARENT,
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

    /// The check of an active button, as `ActiveIcon()`.
    pub fn active_icon() {
        crate::icon::Icon(
            crate::icons::filled::check(),
            None,
            Modifier.size(SegmentedButtonDefaults::ICON_SIZE),
            crate::theme::LOCAL_CONTENT_COLOR.current(),
        );
    }

    /// The active icon when `active`, as `Icon(active)` (shown at once, where Compose fades
    /// and scales it in).
    pub fn icon(active: bool) {
        if active {
            SegmentedButtonDefaults::active_icon();
        }
    }
}

/// A row of segmented buttons each checked or not, as Compose's
/// `MultiChoiceSegmentedButtonRow`: as `SingleChoiceSegmentedButtonRow`, its buttons
/// `CheckableSegmentedButton`s.
#[composable]
pub fn MultiChoiceSegmentedButtonRow(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::SegmentedButtonDefaults::BORDER_WIDTH)] space: Dp,
    content: impl Fn(MultiChoiceSegmentedButtonRowScope) + 'static,
) {
    let policy = compose_runtime::remember_keyed(space, move || {
        Rc::new(SegmentedRowLayout { space }) as Rc<dyn MeasurePolicy>
    });
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || content(MultiChoiceSegmentedButtonRowScope(()))),
    );
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
#[allow(clippy::too_many_arguments)]
pub fn SegmentedButton(
    scope: SingleChoiceSegmentedButtonRowScope,
    selected: bool,
    on_click: impl Fn() + 'static,
    shape: Shape,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::SegmentedButtonDefaults::colors())] colors: SegmentedButtonColors,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::SegmentedButtonDefaults::CONTENT_PADDING)]
    content_padding: compose_ui::PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    label: impl Fn() + 'static,
) {
    let _ = scope;
    let interaction_source = remembered_interaction_source(interaction_source);
    let click = SurfaceClick {
        on_click,
        enabled,
        role: None,
        interaction_source,
        selected: Some(selected),
        checked: None,
        elevations: None,
    };
    let modifier = modifier.semantics(|config| config.role = Some(Role::RadioButton));
    segmented_button(
        selected,
        click,
        shape,
        modifier,
        enabled,
        colors,
        border,
        content_padding,
        icon,
        label,
    );
}

/// A checkable button of a `MultiChoiceSegmentedButtonRow`, as Compose's `SegmentedButton(checked,
/// onCheckedChange, shape, …)` (named apart, D181): as `SegmentedButton`, a click calling
/// `on_checked_change(!checked)`, in the active colors when `checked`; a toggle for
/// accessibility.
#[composable]
#[allow(clippy::too_many_arguments)]
pub fn CheckableSegmentedButton(
    scope: MultiChoiceSegmentedButtonRowScope,
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    shape: Shape,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::SegmentedButtonDefaults::colors())] colors: SegmentedButtonColors,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::SegmentedButtonDefaults::CONTENT_PADDING)]
    content_padding: compose_ui::PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    label: impl Fn() + 'static,
) {
    let _ = scope;
    let interaction_source = remembered_interaction_source(interaction_source);
    let click = SurfaceClick {
        on_click: compose_runtime::callback(move || on_checked_change(!checked)),
        enabled,
        role: None,
        interaction_source,
        selected: None,
        checked: Some(checked),
        elevations: None,
    };
    segmented_button(
        checked,
        click,
        shape,
        modifier,
        enabled,
        colors,
        border,
        content_padding,
        icon,
        label,
    );
}

/// The segmented buttons' body: a clickable surface of `shape` in the active colors when
/// `active`, at least the buttons' minimum size, over its neighbours while interacted with and
/// more while active (Compose's `interactionZIndex`), its icon and label centered.
#[allow(clippy::too_many_arguments)]
fn segmented_button(
    active: bool,
    click: SurfaceClick,
    shape: Shape,
    modifier: Modifier,
    enabled: bool,
    colors: SegmentedButtonColors,
    border: Option<BorderStroke>,
    content_padding: compose_ui::PaddingValues,
    icon: Option<compose_runtime::Callback>,
    label: compose_runtime::Callback,
) {
    let border = border.unwrap_or_else(|| {
        SegmentedButtonDefaults::border_stroke(colors.border_color(enabled, active))
    });
    // Each interaction under way raises it, an active button by 5 more, so its border shows
    // over its neighbours' (read here: an interaction composes it again).
    let source = &click.interaction_source;
    let interactions = [source.is_pressed(), source.is_focused(), source.is_hovered()]
        .into_iter()
        .filter(|on| *on)
        .count() as f32;
    let z_index = interactions + if active { CHECKED_Z_INDEX_FACTOR } else { 0.0 };
    let mut click = Some(click);
    let label_style = MaterialTheme::typography().label_large;
    let coroutines = compose_runtime::remember_coroutine_scope();
    let policy = remember(move || {
        Rc::new(IconLabelLayout { scope: coroutines, offset: Default::default() })
            as Rc<dyn MeasurePolicy>
    });
    let modifier = modifier.z_index(z_index).size_in(compose_ui::SizeSpec::bounds(
        crate::ButtonDefaults::MIN_WIDTH,
        crate::ButtonDefaults::MIN_HEIGHT,
        Dp::UNSPECIFIED,
        Dp::UNSPECIFIED,
    ));
    // As Compose's `SegmentedButtonContent`: the icon and label, centered in the padding.
    let content = compose_runtime::callback(move || {
        let (icon, label, policy) = (icon.clone(), label.clone(), policy.clone());
        Box(
            Modifier.padding(content_padding),
            Alignment::CENTER,
            false,
            compose_runtime::callback(move |_| {
                let (icon, label) = (icon.clone(), label.clone());
                Layout(
                    Modifier,
                    policy.clone(),
                    compose_runtime::callback(move || {
                        let icon = icon.clone();
                        Box(
                            Modifier,
                            Alignment::CENTER,
                            false,
                            compose_runtime::callback(move |_| match &icon {
                                Some(icon) => icon(),
                                None => SegmentedButtonDefaults::icon(active),
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
            }),
        );
    });
    CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(label_style), || {
        surface(
            modifier.clone(),
            shape,
            colors.container_color(enabled, active),
            colors.content_color(enabled, active),
            Dp(0.0),
            Dp(0.0),
            Some(border),
            click.take(),
            content.clone(),
        );
    });
}

/// Between the icon's room and the label, as `IconSpacing`.
const ICON_SPACING: Dp = Dp(8.0);

/// How much an active button rises over its neighbours, as `CheckedZIndexFactor`.
const CHECKED_Z_INDEX_FACTOR: f32 = 5.0;

/// The icon's room (the icon or `ICON_SIZE`, whichever is wider), 8 dp, then the label, as
/// Compose's `SegmentedButtonContentMeasurePolicy`: without an icon, the label moves back by
/// half that room (the standard scheme's fast spatial spring when it changes), so it's
/// centered.
struct IconLabelLayout {
    scope: compose_runtime::CoroutineScope,
    /// The label's animated offset, and the first offset it starts from.
    offset: std::cell::RefCell<(Option<compose_animation::Animatable<f32>>, Option<f32>)>,
}

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
        let icon = scope.measure(*icon, constraints);
        let label = scope.measure(*label, constraints);
        let density = scope.density();
        let icon_size = density.round_to_px(SegmentedButtonDefaults::ICON_SIZE);
        let spacing = density.round_to_px(ICON_SPACING);
        let width = icon_size.max(icon.width) + spacing + label.width;
        let height = label.height;
        let target = if icon.width == 0 { -(icon_size + spacing) / 2 } else { 0 } as f32;
        let offset = {
            let mut state = self.offset.borrow_mut();
            match state.1 {
                None => {
                    state.1 = Some(target);
                    target
                }
                Some(initial) => {
                    let animatable = state
                        .0
                        .get_or_insert_with(|| compose_animation::Animatable::new(initial))
                        .clone();
                    if animatable.target_value() != target {
                        let running = animatable.clone();
                        self.scope.launch(async move {
                            let spec = compose_animation::AnimationSpec::Spring(
                                crate::motion::FAST_SPATIAL,
                            );
                            running.animate_to(target, spec).await;
                        });
                    }
                    // Read while measuring: each frame of the move measures it again.
                    animatable.value()
                }
            }
        };
        scope.place_relative(&icon, 0, (height - icon.height) / 2);
        let x = icon_size + spacing + offset.round() as i32;
        scope.place_relative(&label, x, (height - label.height) / 2);
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "SegmentedButtonContent"
    }
}
