//! `NavigationBar` and `NavigationBarItem`, as Compose's Material 3: a bar of destinations at
//! the bottom of a screen, the selected one's icon in a pill.

use std::rc::Rc;

use compose_foundation::layout::{Arrangement, Box, Row, RowScope};
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
            disabled_icon_color: scheme.on_surface.with_alpha(0.38),
            disabled_text_color: scheme.on_surface.with_alpha(0.38),
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
) {
    let (icon_color, text_color) = match (enabled, selected) {
        (false, _) => (colors.disabled_icon_color, colors.disabled_text_color),
        (true, true) => (colors.selected_icon_color, colors.selected_text_color),
        (true, false) => (colors.unselected_icon_color, colors.unselected_text_color),
    };
    let show_label = label.is_some() && (always_show_label || selected);
    let policy = {
        compose_runtime::remember_keyed(show_label, move || {
            Rc::new(NavigationBarItemLayout { label: show_label }) as Rc<dyn MeasurePolicy>
        })
    };
    let pill = remember(|| Shape::Rounded([compose_ui::CornerSize::Percent(50.0); 4]));
    let indicator = if selected { colors.selected_indicator_color } else { Color::TRANSPARENT };
    let typography = MaterialTheme::typography();
    let modifier = modifier
        .weight(scope, 1.0)
        .fill_max_height()
        .clickable_indicated(enabled, Some(Role::Tab), icon_color, on_click)
        .semantics(false, move |config| config.selected = Some(selected));
    let (icon, label) = (icon.clone(), label.clone());
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            // The pill, then the icon over it, then the label.
            Box(
                Modifier
                    .size_in(SizeSpec::bounds(64.dp(), 32.dp(), 64.dp(), 32.dp()))
                    .background_shape(indicator, pill),
                Alignment::CENTER,
                false,
                compose_runtime::callback(|_| {}),
            );
            let icon = icon.clone();
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(icon_color), || {
                Box(Modifier, Alignment::CENTER, false, compose_runtime::callback(move |_| icon()));
            });
            if show_label && let Some(label) = label.clone() {
                CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(text_color), || {
                    CompositionLocalProvider(
                        LOCAL_TEXT_STYLE.provides(typography.label_medium),
                        || {
                            Box(
                                Modifier,
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
        let label = if self.label {
            measurables.get(2).map(|&label| scope.measure(label, loose))
        } else {
            None
        };
        let gap = density.round_to_px(4.dp());
        let content = pill.height + label.as_ref().map_or(0, |label| gap + label.height);
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
