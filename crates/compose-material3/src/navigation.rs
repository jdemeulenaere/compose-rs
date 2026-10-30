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
    pub const ELEVATION: Dp = Dp(0.0);

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

/// A bar of destinations, as Compose's `NavigationBar`: at least 80 dp high, its items sharing the
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
                    .default_min_size(Dp::UNSPECIFIED, BAR_HEIGHT)
                    .selectable_group(),
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
            selected_text_color: scheme.secondary,
            selected_indicator_color: scheme.secondary_container,
            unselected_icon_color: scheme.on_surface_variant,
            unselected_text_color: scheme.on_surface_variant,
            // The unselected colors faded, as Compose's.
            disabled_icon_color: scheme.on_surface_variant.with_alpha(0.38),
            disabled_text_color: scheme.on_surface_variant.with_alpha(0.38),
        }
    }
}

/// A destination of a `NavigationBar`, as Compose's `NavigationBarItem`: its icon in a 56 by
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
        modifier.default_min_size(Dp::UNSPECIFIED, BAR_HEIGHT).weight(scope, 1.0),
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

/// The bar's height, as `NavigationBarHeight`.
const BAR_HEIGHT: Dp = Dp(80.0);
/// A rail's item, as `NavigationRailItemWidth` and `NavigationRailItemHeight`.
const RAIL_ITEM_WIDTH: Dp = Dp(80.0);
const RAIL_ITEM_HEIGHT: Dp = Dp(56.0);

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
            (ItemKind::Bar, _) => (16.dp(), 4.dp()),
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
    let color_spec = MaterialTheme::motion_scheme().default_effects_spec();
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
            NavigationIcon(icon_color, color_spec.clone(), clear_icon_semantics, icon.clone());
            if let Some(label) = label.clone() {
                let label_modifier = if always_show_label {
                    Modifier
                } else {
                    Modifier.graphics_layer(move |layer| layer.alpha = alpha.get())
                };
                NavigationLabel(text_color, color_spec.clone(), label_modifier, label);
            }
        }),
    );
}

/// As `styledIcon`: its color animated and read here, so a frame of it composes this alone.
#[composable]
#[allow(non_snake_case)]
fn NavigationIcon(
    color: Color,
    spec: compose_animation::AnimationSpec,
    clear_semantics: bool,
    icon: compose_runtime::Callback,
) {
    let color =
        compose_animation::animate_color_as_state(color, spec, "NavigationItemIconColor", None)
            .get();
    let modifier =
        if clear_semantics { Modifier.clear_and_set_semantics(|_| {}) } else { Modifier };
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
        let icon = icon.clone();
        Box(modifier.clone(), Alignment::CENTER, false, compose_runtime::callback(move |_| icon()));
    });
}

/// As `styledLabel`: its color animated and read here, so a frame of it composes this alone.
#[composable]
#[allow(non_snake_case)]
fn NavigationLabel(
    color: Color,
    spec: compose_animation::AnimationSpec,
    modifier: Modifier,
    label: compose_runtime::Callback,
) {
    let color =
        compose_animation::animate_color_as_state(color, spec, "NavigationItemTextColor", None)
            .get();
    let style = MaterialTheme::typography().label_medium.clone();
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
        CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(style.clone()), || {
            let label = label.clone();
            Box(
                modifier.clone(),
                Alignment::CENTER,
                false,
                compose_runtime::callback(move |_| label()),
            );
        });
    });
}

/// As Compose's `NavigationBarItemLayout` and `NavigationRailItemLayout`: the pill's size from the
/// icon's, the indicator on it `progress` of its width, the icon centered on them and the label
/// 4 dp below, all centered across. A label that only shows when selected moves them: at 0 the
/// icon is centered and the label not placed.
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
        let pill_width = icon.width + density.round_to_px(horizontal * 2.0);
        let pill_height = icon.height + density.round_to_px(vertical * 2.0);
        let ripple = scope.measure(measurables[0], Constraints::fixed(pill_width, pill_height));
        let indicator = scope.measure(
            measurables[1],
            Constraints::fixed((pill_width as f32 * progress).round() as i32, pill_height),
        );
        let label = measurables.get(3).map(|&label| scope.measure(label, loose));
        let bar_width = |icon_width: i32| {
            if constraints.has_bounded_width() {
                constraints.max_width
            } else {
                // Asked its intrinsics: `NavigationBarItemToIconMinimumPadding` each side.
                icon_width + 2 * density.round_to_px(44.dp())
            }
        };
        let Some(label) = label else {
            // `placeIcon`: all centered.
            let (width, height) = match self.kind {
                ItemKind::Bar => (
                    bar_width(icon.width),
                    constraints.constrain_height(density.round_to_px(BAR_HEIGHT)),
                ),
                ItemKind::Rail => (
                    constraints.constrain_width(icon.width.max(pill_width).max(indicator.width)),
                    constraints.constrain_height(density.round_to_px(RAIL_ITEM_HEIGHT)),
                ),
            };
            let (ripple_x, ripple_y) = ((width - pill_width) / 2, (height - pill_height) / 2);
            self.pill_position.set(compose_ui::Offset::new(ripple_x as f32, ripple_y as f32));
            scope.place_relative(&indicator, (width - indicator.width) / 2, ripple_y);
            scope.place_relative(&icon, (width - icon.width) / 2, (height - icon.height) / 2);
            scope.place_relative(&ripple, ripple_x, ripple_y);
            return scope.layout(width, height);
        };
        // `placeLabelAndIcon`, in its float arithmetic, rounded where it places.
        let vertical_px = density.to_px(vertical);
        let gap = density.to_px(4.dp());
        let content = icon.height as f32 + vertical_px + gap + label.height as f32;
        let padding = ((constraints.min_height as f32 - content) / 2.0).max(vertical_px);
        let height = content + padding * 2.0;
        let selected_icon_y = padding;
        let unselected_icon_y = if self.always_show_label {
            selected_icon_y
        } else {
            (height - icon.height as f32) / 2.0
        };
        let offset = (unselected_icon_y - selected_icon_y) * (1.0 - progress);
        let label_y = selected_icon_y + icon.height as f32 + vertical_px + gap;
        let width = match self.kind {
            ItemKind::Bar => bar_width(icon.width),
            ItemKind::Rail => {
                constraints.constrain_width(icon.width.max(label.width).max(indicator.width))
            }
        };
        let ripple_y = selected_icon_y - vertical_px;
        // The bar rounds its padding here, the rail doesn't, as each of Compose's does.
        let indicator_y = match self.kind {
            ItemKind::Bar => selected_icon_y - density.round_to_px(vertical) as f32,
            ItemKind::Rail => ripple_y,
        };
        let ripple_x = (width - pill_width) / 2;
        let placed_ripple_y = (ripple_y + offset).round() as i32;
        self.pill_position.set(compose_ui::Offset::new(ripple_x as f32, placed_ripple_y as f32));
        scope.place_relative(
            &indicator,
            (width - indicator.width) / 2,
            (indicator_y + offset).round() as i32,
        );
        // Unplaced while it doesn't show, as Compose's: it still names the item, its semantics
        // merging all the same (`merge_into`).
        if self.always_show_label || progress != 0.0 {
            scope.place_relative(
                &label,
                (width - label.width) / 2,
                (label_y + offset).round() as i32,
            );
        }
        scope.place_relative(
            &icon,
            (width - icon.width) / 2,
            (selected_icon_y + offset).round() as i32,
        );
        scope.place_relative(&ripple, ripple_x, placed_ripple_y);
        scope.layout(width, height.round() as i32)
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
/// 32 dp pill when selected, its label (label-medium) 4 dp below; at least 80 dp wide, 56 dp high
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
        modifier.default_min_size(Dp::UNSPECIFIED, RAIL_ITEM_HEIGHT).size_in(SizeSpec::bounds(
            RAIL_ITEM_WIDTH,
            Dp::UNSPECIFIED,
            Dp::UNSPECIFIED,
            Dp::UNSPECIFIED,
        )),
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
