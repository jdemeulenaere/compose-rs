//! `NavigationBar` and `NavigationBarItem`, `NavigationRail` and `NavigationRailItem`, as
//! Compose's Material 3: a bar of destinations at the bottom of a screen or a rail at its side,
//! the selected one's icon in a pill.

use compose_ui::ClickableOptions;
use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::{
    Arrangement, Box, Column, ColumnScope, Row, RowScope, WindowInsets, WindowInsetsModifiers,
    WindowInsetsSides,
};
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

    /// As `windowInsets`: the system bars and a cutout, at the bottom and the sides.
    pub fn window_insets() -> WindowInsets {
        crate::scaffold::system_bars_for_visual_components()
            .only(WindowInsetsSides::HORIZONTAL + WindowInsetsSides::BOTTOM)
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
    #[default(crate::NavigationBarDefaults::window_insets())] window_insets: WindowInsets,
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
                Modifier
                    .fill_max_width()
                    .window_insets_padding(window_insets.clone())
                    .height(80.dp()),
                Arrangement::spaced_by(8.dp()).into(),
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
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    navigation_item(
        modifier.weight(scope, 1.0).fill_max_height(),
        ItemKind::Bar,
        selected,
        on_click,
        icon,
        enabled,
        label,
        always_show_label,
        colors,
        interaction_source,
    );
}

/// Which of the two an item is: their measures differ.
#[derive(Clone, Copy, PartialEq)]
enum ItemKind {
    Bar,
    Rail,
}

impl ItemKind {
    /// Around the icon, the indicator's width, and its height, as `IndicatorHorizontalPadding`
    /// and `IndicatorVerticalPadding`, a rail's item without a label a circle.
    fn indicator_padding(self, has_label: bool) -> (Dp, Dp) {
        match (self, has_label) {
            (ItemKind::Bar, _) => (20.dp(), 4.dp()),
            (ItemKind::Rail, true) => (16.dp(), 4.dp()),
            (ItemKind::Rail, false) => (16.dp(), 16.dp()),
        }
    }
}

/// A navigation bar's or rail's item: clickable (a tab, selected or not, its press shown on the
/// pill only), the pill's ripple, the indicator on it, widening and fading in as it's selected,
/// the icon over them, the label, colors following the selection, on the theme's motion scheme.
#[allow(clippy::too_many_arguments)]
fn navigation_item(
    modifier: Modifier,
    kind: ItemKind,
    selected: bool,
    on_click: compose_runtime::Callback,
    icon: compose_runtime::Callback,
    enabled: bool,
    label: Option<compose_runtime::Callback>,
    always_show_label: bool,
    colors: NavigationBarItemColors,
    interaction: compose_ui::MutableInteractionSource,
) {
    let (icon_color, text_color) = match (enabled, selected) {
        (false, _) => (colors.disabled_icon_color, colors.disabled_text_color),
        (true, true) => (colors.selected_icon_color, colors.selected_text_color),
        (true, false) => (colors.unselected_icon_color, colors.unselected_text_color),
    };
    let motion = MaterialTheme::motion_scheme();
    let target = if selected { 1.0 } else { 0.0 };
    let alpha = compose_animation::animate_float_as_state(
        target,
        motion.default_effects_spec(),
        compose_animation::Spring::DEFAULT_DISPLACEMENT_THRESHOLD,
        "NavigationItemIndicatorAlpha",
        None,
    );
    let size = compose_animation::animate_float_as_state(
        target,
        motion.fast_spatial_spec(),
        compose_animation::Spring::DEFAULT_DISPLACEMENT_THRESHOLD,
        "NavigationItemIndicatorSize",
        None,
    );
    let icon_color = compose_animation::animate_color_as_state(
        icon_color,
        motion.default_effects_spec(),
        "NavigationItemIconColor",
        None,
    );
    let text_color = compose_animation::animate_color_as_state(
        text_color,
        motion.default_effects_spec(),
        "NavigationItemTextColor",
        None,
    );
    let pill_position = remember(|| Rc::new(Cell::new(compose_ui::Offset::ZERO)));
    let has_label = label.is_some();
    let policy = compose_runtime::remember_keyed((has_label, always_show_label), {
        let pill_position = pill_position.clone();
        move || {
            Rc::new(NavigationItemLayout {
                kind,
                has_label,
                always_show_label,
                progress: size,
                pill_position: pill_position.clone(),
            }) as Rc<dyn MeasurePolicy>
        }
    });
    let pill = remember(|| Shape::Rounded([compose_ui::CornerSize::Percent(50.0); 4]));
    let indicator_color = colors.selected_indicator_color;
    let typography = MaterialTheme::typography();
    // The press, focus and hover show on the pill only, as Compose's: the item's
    // `selectable(indication = null)`, its interactions mapped into the pill's coordinates for
    // the pill's own ripple.
    let mapped = compose_runtime::remember_keyed(interaction.clone(), {
        let (source, position) = (interaction.clone(), pill_position.clone());
        move || {
            Rc::new(crate::ripple::MappedInteractionSource::new(source.as_source(), move || {
                position.get()
            }))
        }
    });
    let modifier = modifier
        .clickable_with(
            ClickableOptions {
                enabled,
                role: Some(Role::Tab),
                indication: compose_ui::Indication::NONE,
                interaction_source: Some(interaction.clone()),
                ..ClickableOptions::DEFAULT
            },
            on_click,
        )
        .semantics(move |config| config.set_selected(selected));
    let (icon, label) = (icon.clone(), label.clone());
    // A label shown names the item: the icon's description isn't read too.
    let clear_icon_semantics = has_label && (always_show_label || selected);
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            // The ripple apart from the indicator, whose width would disturb it, as Compose's.
            Box(
                Modifier
                    .clip(pill.clone())
                    .indication(mapped.source(), crate::ripple::shaped_ripple(pill.clone())),
                Alignment::CENTER,
                false,
                compose_runtime::callback(|_| {}),
            );
            Box(
                Modifier
                    .graphics_layer(move |layer| layer.alpha = alpha.get())
                    .background_with(indicator_color, pill.clone()),
                Alignment::CENTER,
                false,
                compose_runtime::callback(|_| {}),
            );
            let icon = icon.clone();
            let icon_modifier = if clear_icon_semantics {
                Modifier.clear_and_set_semantics(|_| {})
            } else {
                Modifier
            };
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(icon_color.get()), || {
                Box(
                    icon_modifier.clone(),
                    Alignment::CENTER,
                    false,
                    compose_runtime::callback(move |_| icon()),
                );
            });
            if let Some(label) = label.clone() {
                CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(text_color.get()), || {
                    CompositionLocalProvider(
                        LOCAL_TEXT_STYLE.provides(typography.label_medium.clone()),
                        || {
                            let label_modifier = if always_show_label {
                                Modifier
                            } else {
                                Modifier.graphics_layer(move |layer| layer.alpha = alpha.get())
                            };
                            Box(
                                label_modifier,
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

/// As Compose's `NavigationBarItemLayout` and `NavigationRailItemLayout`: the pill's size from the
/// icon's, the indicator on it `progress` of its width, the icon centered on them and the label
/// 4 dp below, all centered across. A label that only shows when selected moves them: at 0 the
/// icon is centered and the label transparent.
struct NavigationItemLayout {
    kind: ItemKind,
    has_label: bool,
    always_show_label: bool,
    /// 0 unselected, 1 selected: read while measuring, each frame of a selection laying it out.
    progress: compose_runtime::State<f32>,
    /// Where the pill is placed, for its ripple's coordinates.
    pill_position: Rc<Cell<compose_ui::Offset>>,
}

impl MeasurePolicy for NavigationItemLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        // It may go below 0 on a bouncy spring.
        let progress = self.progress.get().max(0.0);
        let loose = constraints.loosen();
        let icon = scope.measure(measurables[2], loose);
        let (horizontal, vertical) = self.kind.indicator_padding(self.has_label);
        let (horizontal, vertical) =
            (density.round_to_px(horizontal), density.round_to_px(vertical));
        let pill_width = icon.width + 2 * horizontal;
        let pill_height = icon.height + 2 * vertical;
        let ripple = scope.measure(measurables[0], Constraints::fixed(pill_width, pill_height));
        let indicator = scope.measure(
            measurables[1],
            Constraints::fixed((pill_width as f32 * progress).round() as i32, pill_height),
        );
        let label = measurables.get(3).map(|&label| scope.measure(label, loose));
        let gap = density.round_to_px(4.dp());
        let content = icon.height + vertical + label.as_ref().map_or(0, |it| gap + it.height);
        // The room around a label's content, and the item's size.
        let (width, height) = match self.kind {
            ItemKind::Bar => {
                // Unbounded (asked its intrinsics), as Compose's: the icon and 44 dp each side
                // (`NavigationBarItemToIconMinimumPadding`); the bar's 80 dp high, or with a label
                // its content and the indicator's 4 dp above and below (D1207).
                let width = if constraints.has_bounded_width() {
                    constraints.max_width
                } else {
                    icon.width + 2 * density.round_to_px(44.dp())
                };
                let height = if constraints.has_bounded_height() {
                    constraints.max_height
                } else if label.is_some() {
                    content + 2 * vertical
                } else {
                    constraints.constrain_height(density.round_to_px(80.dp()))
                };
                (width, height)
            }
            ItemKind::Rail => {
                let minimum = density.round_to_px(56.dp());
                let height = match &label {
                    Some(_) => constraints
                        .constrain_height(content + 2 * ((minimum - content) / 2).max(vertical)),
                    None => constraints.constrain_height(minimum),
                };
                let width = constraints.constrain_width(
                    pill_width.max(minimum).max(label.as_ref().map_or(0, |it| it.width)),
                );
                (width, height)
            }
        };
        let center = |child_width: i32| (width - child_width) / 2;
        let (icon_y, label_y, offset) = match &label {
            None => ((height - icon.height) / 2, 0, 0),
            Some(_) => {
                let selected_icon_y = ((height - content) / 2).max(vertical);
                let unselected_icon_y = if self.always_show_label {
                    selected_icon_y
                } else {
                    (height - icon.height) / 2
                };
                let distance = (unselected_icon_y - selected_icon_y) as f32;
                let offset = (distance * (1.0 - progress)).round() as i32;
                (selected_icon_y, selected_icon_y + icon.height + vertical + gap, offset)
            }
        };
        let pill_y = icon_y - vertical + offset;
        self.pill_position.set(compose_ui::Offset::new(center(pill_width) as f32, pill_y as f32));
        scope.place_relative(&indicator, center(indicator.width), pill_y);
        // A hidden label is placed all the same, transparent, where Compose's isn't: it names the
        // item for screen readers there, its unplaced node still in the semantics, and unplaced
        // nodes are absent from compose-rs's (D1027).
        if let Some(label) = &label {
            scope.place_relative(label, center(label.width), label_y + offset);
        }
        scope.place_relative(&icon, center(icon.width), icon_y + offset);
        scope.place_relative(&ripple, center(pill_width), pill_y);
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        match self.kind {
            ItemKind::Bar => "NavigationBarItem",
            ItemKind::Rail => "NavigationRailItem",
        }
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

    /// As `windowInsets`: the system bars and a cutout, at the top, the bottom and the start.
    pub fn window_insets() -> WindowInsets {
        crate::scaffold::system_bars_for_visual_components()
            .only(WindowInsetsSides::VERTICAL + WindowInsetsSides::START)
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
    #[default(crate::NavigationRailDefaults::window_insets())] window_insets: WindowInsets,
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
                    .window_insets_padding(window_insets.clone())
                    .size_in(SizeSpec::bounds(
                        80.dp(),
                        Dp::UNSPECIFIED,
                        Dp::UNSPECIFIED,
                        Dp::UNSPECIFIED,
                    ))
                    .padding((Dp(0.0), 4.dp()))
                    .selectable_group()
                    .semantics(|config| config.set_is_traversal_group(true)),
                Arrangement::spaced_by(4.dp()).into(),
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
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    navigation_item(
        modifier,
        ItemKind::Rail,
        selected,
        on_click,
        icon,
        enabled,
        label,
        always_show_label,
        colors,
        interaction_source,
    );
}
