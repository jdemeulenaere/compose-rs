//! `NavigationBar` and `NavigationBarItem`, `NavigationRail` and `NavigationRailItem`, as
//! Compose's Material 3: a bar of destinations at the bottom of a screen or a rail at its side,
//! the selected one's icon in a pill.

use std::rc::Rc;

use compose_foundation::layout::{Arrangement, Box, Column, ColumnScope, Row, RowScope};
use compose_runtime::{CompositionLocalProvider, composable, remember};
use compose_ui::{
    Alignment, Color, Constraints, Dp, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier, Role, Shape, SizeSpec, UnitExt,
};

use crate::components::Surface;
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

/// As Compose's `NavigationBarDefaults`.
pub struct NavigationBarDefaults;

impl NavigationBarDefaults {
    /// Its tonal elevation, as `NavigationBarDefaults.Elevation`.
    pub const ELEVATION: Dp = Dp(3.0);

    /// Its color, as `NavigationBarDefaults.containerColor`.
    pub fn container_color() -> Color {
        MaterialTheme::color_scheme().surface_container
    }
}

/// A bar of destinations, as Compose's `NavigationBar`: 80 dp high, its items sharing the
/// width, 8 dp apart.
#[composable]
#[allow(non_snake_case)]
pub fn NavigationBar(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::NavigationBarDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::NavigationBarDefaults::ELEVATION)] tonal_elevation: Dp,
    content: impl Fn(RowScope) + 'static,
) {
    let content_color =
        content_color.or(MaterialTheme::color_scheme().content_color_for(container_color));
    Surface(
        modifier,
        compose_ui::RECTANGLE_SHAPE,
        container_color,
        content_color,
        tonal_elevation,
        Dp(0.0),
        None,
        compose_runtime::callback(move || {
            Row(
                Modifier.fill_max_width().height(80.dp()),
                Arrangement::spaced_by(8.dp()),
                Alignment::CENTER_VERTICALLY,
                content.clone(),
            );
        }),
    );
}

/// A navigation bar item's colors, as Compose's `NavigationBarItemColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NavigationBarItemColors {
    pub selected_icon_color: Color,
    pub selected_text_color: Color,
    pub selected_indicator_color: Color,
    pub unselected_icon_color: Color,
    pub unselected_text_color: Color,
    pub disabled_icon_color: Color,
    pub disabled_text_color: Color,
}

impl compose_runtime::ComposeArg for NavigationBarItemColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `NavigationBarItemDefaults`.
pub struct NavigationBarItemDefaults;

impl NavigationBarItemDefaults {
    /// From the theme, as `NavigationBarItemDefaults.colors()`.
    pub fn colors() -> NavigationBarItemColors {
        let scheme = MaterialTheme::color_scheme();
        NavigationBarItemColors {
            selected_icon_color: scheme.on_secondary_container,
            selected_text_color: scheme.on_surface,
            selected_indicator_color: scheme.secondary_container,
            unselected_icon_color: scheme.on_surface_variant,
            unselected_text_color: scheme.on_surface_variant,
            // The unselected colors faded, as Compose's.
            disabled_icon_color: scheme.on_surface_variant.with_alpha(0.38),
            disabled_text_color: scheme.on_surface_variant.with_alpha(0.38),
        }
    }
}

/// A destination of a `NavigationBar`, as Compose's `NavigationBarItem`: its icon in a 64 by
/// 32 dp pill when selected, its label (label-medium) 4 dp below, centered in its share of the
/// bar; clicking selects it.
#[composable]
#[allow(non_snake_case)]
pub fn NavigationBarItem(
    scope: RowScope,
    selected: bool,
    on_click: impl Fn() + 'static,
    icon: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] label: Option<impl Fn() + 'static>,
    #[default(true)] always_show_label: bool,
    #[default(crate::NavigationBarItemDefaults::colors())] colors: NavigationBarItemColors,
    #[default(None)] interaction_source: Option<compose_ui::MutableInteractionSource>,
) {
    let show_label = label.is_some() && (always_show_label || selected);
    let policy = {
        compose_runtime::remember_keyed(show_label, move || {
            Rc::new(NavigationBarItemLayout { label: show_label }) as Rc<dyn MeasurePolicy>
        })
    };
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    navigation_item(
        modifier.weight(scope, 1.0).fill_max_height(),
        policy,
        selected,
        on_click,
        icon,
        enabled,
        label,
        show_label,
        colors,
        interaction_source,
        (64.dp(), 32.dp()),
    );
}

/// A navigation bar's or rail's item: clickable (a tab, selected or not, its press shown on
/// the pill only), the pill (the indicator when selected), the icon over it, the label.
#[allow(clippy::too_many_arguments)]
fn navigation_item(
    modifier: Modifier,
    policy: Rc<dyn MeasurePolicy>,
    selected: bool,
    on_click: compose_runtime::Callback,
    icon: compose_runtime::Callback,
    enabled: bool,
    label: Option<compose_runtime::Callback>,
    show_label: bool,
    colors: NavigationBarItemColors,
    interaction: compose_ui::MutableInteractionSource,
    pill_size: (Dp, Dp),
) {
    let (icon_color, text_color) = match (enabled, selected) {
        (false, _) => (colors.disabled_icon_color, colors.disabled_text_color),
        (true, true) => (colors.selected_icon_color, colors.selected_text_color),
        (true, false) => (colors.unselected_icon_color, colors.unselected_text_color),
    };
    let pill = remember(|| Shape::Rounded([compose_ui::CornerSize::Percent(50.0); 4]));
    let indicator = if selected { colors.selected_indicator_color } else { Color::TRANSPARENT };
    let typography = MaterialTheme::typography();
    // The press, focus and hover show on the pill only, as Compose's
    // (`selectable(indication = null)`, the pill's own ripple).
    let modifier = modifier
        .clickable_interaction(
            enabled,
            Some(Role::Tab),
            Color::UNSPECIFIED,
            interaction.clone(),
            on_click,
        )
        .semantics(false, move |config| config.selected = Some(selected));
    let (icon, label) = (icon.clone(), label.clone());
    let (pill_width, pill_height) = pill_size;
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            // The pill, then the icon over it, then the label.
            let (pressed, pill_shape) = (interaction.clone(), pill);
            Box(
                Modifier
                    .size_in(SizeSpec::bounds(pill_width, pill_height, pill_width, pill_height))
                    .background_shape(indicator, pill)
                    .draw_behind(move |scope| {
                        // Read while drawing: a press, focus or hover draws again only.
                        // Material's state layers: pressed and focused 10 %, hovered 8 %.
                        let alpha = if pressed.is_pressed() || pressed.is_focused() {
                            Some(0.1)
                        } else if pressed.is_hovered() {
                            Some(0.08)
                        } else {
                            None
                        };
                        if let Some(alpha) = alpha
                            && enabled
                        {
                            scope.draw_shape(&pill_shape, icon_color.with_alpha(alpha));
                        }
                    }),
                Alignment::CENTER,
                false,
                compose_runtime::callback(|_| {}),
            );
            let icon = icon.clone();
            // A label shown names the item: the icon's description isn't read too.
            let icon_modifier =
                if show_label { Modifier.clear_and_set_semantics(|_| {}) } else { Modifier };
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(icon_color), || {
                Box(
                    icon_modifier.clone(),
                    Alignment::CENTER,
                    false,
                    compose_runtime::callback(move |_| icon()),
                );
            });
            // A hidden label is still there, transparent: it names the item for screen readers.
            if let Some(label) = label.clone() {
                CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(text_color), || {
                    CompositionLocalProvider(
                        LOCAL_TEXT_STYLE.provides(typography.label_medium),
                        || {
                            Box(
                                if show_label { Modifier } else { Modifier.alpha(0.0) },
                                Alignment::CENTER,
                                false,
                                compose_runtime::callback({
                                    let label = label.clone();
                                    move |_| label()
                                }),
                            );
                        },
                    );
                });
            }
        }),
    );
}

/// The pill, the icon centered on it, and the label 4 dp below, all centered across; the
/// whole centered down the item, as Compose's `NavigationBarItemLayout`.
struct NavigationBarItemLayout {
    label: bool,
}

impl MeasurePolicy for NavigationBarItemLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let loose = constraints.loosen();
        let width = constraints.max_width;
        let height = constraints.max_height;
        let pill = scope.measure(measurables[0], loose);
        let icon = scope.measure(measurables[1], loose);
        let label = measurables.get(2).map(|&label| scope.measure(label, loose));
        let gap = density.round_to_px(4.dp());
        // A hidden label takes no room: the icon centered alone, the label (transparent) under.
        let shown = label.as_ref().filter(|_| self.label);
        let content = pill.height + shown.map_or(0, |label| gap + label.height);
        let top = (height - content).max(0) / 2;
        let center = |child_width: i32| (width - child_width) / 2;
        scope.place_relative(&pill, center(pill.width), top);
        scope.place_relative(&icon, center(icon.width), top + (pill.height - icon.height) / 2);
        if let Some(label) = &label {
            scope.place_relative(label, center(label.width), top + pill.height + gap);
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "NavigationBarItem"
    }
}

// ---- NavigationRail --------------------------------------------------------------------------

/// As Compose's `NavigationRailDefaults`.
pub struct NavigationRailDefaults;

impl NavigationRailDefaults {
    /// The surface, as `ContainerColor`.
    pub fn container_color() -> Color {
        MaterialTheme::color_scheme().surface
    }

    /// As `windowInsets`: none until hosts report them.
    pub fn window_insets() -> compose_ui::PaddingValues {
        compose_ui::PaddingValues::ZERO
    }
}

/// A navigation rail item's colors, as Compose's `NavigationRailItemColors` (the navigation
/// bar's roles).
pub type NavigationRailItemColors = NavigationBarItemColors;

/// As Compose's `NavigationRailItemDefaults`.
pub struct NavigationRailItemDefaults;

impl NavigationRailItemDefaults {
    /// From the theme, as `NavigationRailItemDefaults.colors()`.
    pub fn colors() -> NavigationRailItemColors {
        NavigationBarItemDefaults::colors()
    }
}

/// A rail of destinations at the side of a screen, as Compose's `NavigationRail`: at least 80 dp
/// wide, as tall as allowed, its optional header (a FAB, a menu) then 8 dp, then the items,
/// 4 dp apart and from the ends, centered across.
#[composable]
#[allow(non_snake_case)]
pub fn NavigationRail(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::NavigationRailDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(None)] header: Option<impl Fn(ColumnScope) + 'static>,
    #[default(crate::NavigationRailDefaults::window_insets())]
    window_insets: compose_ui::PaddingValues,
    content: impl Fn(ColumnScope) + 'static,
) {
    Surface(
        modifier,
        compose_ui::RECTANGLE_SHAPE,
        container_color,
        content_color,
        Dp(0.0),
        Dp(0.0),
        None,
        compose_runtime::callback(move || {
            let (header, content) = (header.clone(), content.clone());
            Column(
                Modifier
                    .fill_max_height()
                    .padding_values(window_insets)
                    .size_in(SizeSpec::bounds(
                        80.dp(),
                        Dp::UNSPECIFIED,
                        Dp::UNSPECIFIED,
                        Dp::UNSPECIFIED,
                    ))
                    .padding_horizontal_vertical(Dp(0.0), 4.dp()),
                Arrangement::spaced_by(4.dp()),
                Alignment::CENTER_HORIZONTALLY,
                compose_runtime::callback(move |column: ColumnScope| {
                    if let Some(header) = header.clone() {
                        header(column);
                        compose_foundation::layout::Spacer(Modifier.height(8.dp()));
                    }
                    content(column);
                }),
            );
        }),
    );
}

/// A destination of a `NavigationRail`, as Compose's `NavigationRailItem`: its icon on a 56 by
/// 32 dp pill when selected, its label (label-medium) 4 dp below; 56 dp wide, 56 dp high
/// without a label (the pill a 56 dp circle), 60 with one; clicking selects it.
#[composable]
#[allow(non_snake_case)]
pub fn NavigationRailItem(
    selected: bool,
    on_click: impl Fn() + 'static,
    icon: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] label: Option<impl Fn() + 'static>,
    #[default(true)] always_show_label: bool,
    #[default(crate::NavigationRailItemDefaults::colors())] colors: NavigationRailItemColors,
    #[default(None)] interaction_source: Option<compose_ui::MutableInteractionSource>,
) {
    let show_label = label.is_some() && (always_show_label || selected);
    let policy = compose_runtime::remember_keyed(show_label, move || {
        Rc::new(NavigationRailItemLayout { label: show_label }) as Rc<dyn MeasurePolicy>
    });
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    navigation_item(
        modifier,
        policy,
        selected,
        on_click,
        icon,
        enabled,
        label,
        show_label,
        colors,
        interaction_source,
        (56.dp(), if show_label { 32.dp() } else { 56.dp() }),
    );
}

/// The pill, the icon centered on it, and the label 4 dp below, centered across; the whole
/// centered down at least 56 dp (4 dp at least above and below), as Compose's
/// `NavigationRailItem`'s layout.
struct NavigationRailItemLayout {
    label: bool,
}

impl MeasurePolicy for NavigationRailItemLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let loose = constraints.loosen();
        let pill = scope.measure(measurables[0], loose);
        let icon = scope.measure(measurables[1], loose);
        let label = measurables.get(2).map(|&label| scope.measure(label, loose));
        let minimum = density.round_to_px(56.dp());
        let gap = density.round_to_px(4.dp());
        let shown = label.as_ref().filter(|_| self.label);
        let content = pill.height + shown.map_or(0, |label| gap + label.height);
        let padding = ((minimum - content) / 2).max(if shown.is_some() { gap } else { 0 });
        let height = constraints.constrain_height(content + 2 * padding);
        let width = constraints.constrain_width(
            pill.width.max(minimum).max(label.as_ref().map_or(0, |label| label.width)),
        );
        let center = |child_width: i32| (width - child_width) / 2;
        let top = (height - content) / 2;
        scope.place_relative(&pill, center(pill.width), top);
        scope.place_relative(&icon, center(icon.width), top + (pill.height - icon.height) / 2);
        if let Some(label) = &label {
            scope.place_relative(label, center(label.width), top + pill.height + gap);
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "NavigationRailItem"
    }
}
