//! Screen structure, as Compose's: `Scaffold`, `TopAppBar`, `Card` and dividers.

use std::rc::Rc;

use compose_foundation::BorderStroke;
use compose_foundation::layout::{Box, Column, ColumnScope, Row, RowScope};
use compose_runtime::{
    CompositionLocalProvider, MutableState, composable, mutable_state_of, remember,
};
use compose_ui::{
    Alignment, Color, Constraints, Dp, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier, PaddingValues, Role, Shape, UnitExt,
};

use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

// ---- Dividers --------------------------------------------------------------------------------

/// A horizontal line, as Compose's `HorizontalDivider`: as wide as allowed.
#[composable]
pub fn HorizontalDivider(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Dp(1.0))] thickness: Dp,
    #[default(crate::MaterialTheme::color_scheme().outline_variant)] color: Color,
) {
    compose_foundation::layout::Spacer(
        modifier.fill_max_width().height(thickness).background(color),
    );
}

/// A vertical line, as Compose's `VerticalDivider`: as tall as allowed.
#[composable]
pub fn VerticalDivider(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Dp(1.0))] thickness: Dp,
    #[default(crate::MaterialTheme::color_scheme().outline_variant)] color: Color,
) {
    compose_foundation::layout::Spacer(
        modifier.fill_max_height().width(thickness).background(color),
    );
}

// ---- Card ------------------------------------------------------------------------------------

/// A card's colors, as Compose's `CardColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CardColors {
    pub container_color: Color,
    pub content_color: Color,
}

impl compose_runtime::ComposeArg for CardColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `CardDefaults`.
pub struct CardDefaults;

impl CardDefaults {
    /// A filled card's, as `CardDefaults.cardColors()`.
    pub fn card_colors() -> CardColors {
        let scheme = MaterialTheme::color_scheme();
        CardColors {
            container_color: scheme.surface_container_highest,
            content_color: scheme.on_surface,
        }
    }

    pub fn shape() -> Shape {
        MaterialTheme::shapes().medium
    }
}

/// A filled card, as Compose's `Card`: its content in a column on the medium shape; clickable
/// when `on_click` is given (Compose's `Card(onClick)` overload).
#[composable]
pub fn Card(
    #[default(None)] on_click: Option<impl Fn() + 'static>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::CardDefaults::shape())] shape: Shape,
    #[default(crate::CardDefaults::card_colors())] colors: CardColors,
    #[default(None)] border: Option<BorderStroke>,
    content: impl Fn(ColumnScope) + 'static,
) {
    let mut surface = modifier.background_shape(colors.container_color, shape).clip(shape);
    if let Some(border) = border {
        surface = surface.border(border.width, border.color, shape);
    }
    if let Some(on_click) = on_click {
        surface = surface.clickable_indicated(
            enabled,
            Some(Role::Button),
            colors.content_color,
            on_click,
        );
    }
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(colors.content_color), || {
        Column(
            surface.clone(),
            compose_foundation::layout::Arrangement::TOP,
            compose_ui::Alignment::START,
            content.clone(),
        );
    });
}

// ---- TopAppBar -------------------------------------------------------------------------------

/// A top app bar's colors, as Compose's `TopAppBarColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TopAppBarColors {
    pub container_color: Color,
    pub navigation_icon_content_color: Color,
    pub title_content_color: Color,
    pub action_icon_content_color: Color,
}

impl compose_runtime::ComposeArg for TopAppBarColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `TopAppBarDefaults`.
pub struct TopAppBarDefaults;

impl TopAppBarDefaults {
    pub fn top_app_bar_colors() -> TopAppBarColors {
        let scheme = MaterialTheme::color_scheme();
        TopAppBarColors {
            container_color: scheme.surface,
            navigation_icon_content_color: scheme.on_surface,
            title_content_color: scheme.on_surface,
            action_icon_content_color: scheme.on_surface_variant,
        }
    }
}

/// A small top app bar, as Compose's `TopAppBar`: 64 dp high, the navigation icon at the start,
/// the title in the title-large style, the actions at the end.
#[composable]
pub fn TopAppBar(
    title: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] navigation_icon: Option<impl Fn() + 'static>,
    #[default(|_| {})] actions: impl Fn(RowScope) + 'static,
    #[default(crate::TopAppBarDefaults::top_app_bar_colors())] colors: TopAppBarColors,
) {
    let style = MaterialTheme::typography().title_large;
    let has_navigation = navigation_icon.is_some();
    Row(
        modifier
            .background(colors.container_color)
            .fill_max_width()
            .height(64.dp())
            .padding_horizontal_vertical(4.dp(), 0.dp()),
        compose_foundation::layout::Arrangement::Start,
        Alignment::CENTER_VERTICALLY,
        compose_runtime::callback(move |row: RowScope| {
            if let Some(navigation_icon) = navigation_icon.clone() {
                CompositionLocalProvider(
                    LOCAL_CONTENT_COLOR.provides(colors.navigation_icon_content_color),
                    move || navigation_icon(),
                );
            }
            // The title starts 16 dp in (the bar's 4 and 12), or right after the navigation
            // icon, as Compose's `TopAppBarTitleInset`.
            let inset = if has_navigation { 0.dp() } else { 12.dp() };
            let title = title.clone();
            CompositionLocalProvider(
                LOCAL_CONTENT_COLOR.provides(colors.title_content_color),
                || {
                    CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(style), || {
                        Box(
                            Modifier.weight(row, 1.0).padding_horizontal_vertical(inset, 0.dp()),
                            Alignment::CENTER_START,
                            false,
                            compose_runtime::callback({
                                let title = title.clone();
                                move |_| title()
                            }),
                        );
                    });
                },
            );
            let actions = actions.clone();
            CompositionLocalProvider(
                LOCAL_CONTENT_COLOR.provides(colors.action_icon_content_color),
                || {
                    Row(
                        Modifier,
                        compose_foundation::layout::Arrangement::End,
                        Alignment::CENTER_VERTICALLY,
                        actions.clone(),
                    );
                },
            );
        }),
    );
}

// ---- Scaffold --------------------------------------------------------------------------------

/// The basic screen layout, as Compose's `Scaffold`: the top bar, the content (given the padding
/// that keeps it clear of the bars), the bottom bar, and the floating action button above the
/// bottom bar at the end, 16 dp in.
///
/// Compose measures the bars before composing the content (subcomposition); here the content
/// reads the padding as state its measurement writes, composed again within the same frame.
#[composable]
pub fn Scaffold(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(|| {})] top_bar: impl Fn() + 'static,
    #[default(|| {})] bottom_bar: impl Fn() + 'static,
    #[default(|| {})] floating_action_button: impl Fn() + 'static,
    #[default(crate::MaterialTheme::color_scheme().background)] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    content: impl Fn(PaddingValues) + 'static,
) {
    let content_color =
        content_color.or(MaterialTheme::color_scheme().content_color_for(container_color));
    let padding = remember(|| mutable_state_of(PaddingValues::ZERO));
    let policy = remember(|| Rc::new(ScaffoldLayout { padding }) as Rc<dyn MeasurePolicy>);
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(content_color), || {
        let (top_bar, bottom_bar, fab, content) =
            (top_bar.clone(), bottom_bar.clone(), floating_action_button.clone(), content.clone());
        Layout(
            modifier.background(container_color),
            policy.clone(),
            compose_runtime::callback(move || {
                let slot = |content: compose_runtime::Callback| {
                    Box(
                        Modifier,
                        Alignment::TOP_START,
                        false,
                        compose_runtime::callback(move |_| content()),
                    );
                };
                slot(top_bar.clone());
                let content = content.clone();
                slot(compose_runtime::callback(move || content(padding.get())));
                slot(bottom_bar.clone());
                slot(fab.clone());
            }),
        );
    });
}

/// Children: the top bar, the content, the bottom bar, the floating action button.
struct ScaffoldLayout {
    padding: MutableState<PaddingValues>,
}

impl MeasurePolicy for ScaffoldLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let width = constraints.max_width;
        let height = constraints.max_height;
        let [top, content, bottom, fab] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let loose = constraints.loosen();
        let top = scope.measure(*top, loose);
        let bottom = scope.measure(*bottom, loose);
        let fab = scope.measure(*fab, loose);
        // The content's padding: the bars' heights. Written as state the content reads, which
        // composes it again (within the frame) when they change.
        let density = scope.density();
        let padding = PaddingValues::new(
            Dp(0.0),
            Dp(top.height as f32 / density.density),
            Dp(0.0),
            Dp(bottom.height as f32 / density.density),
        );
        if self.padding.get() != padding {
            self.padding.set(padding);
        }
        let content = scope.measure(*content, Constraints::fixed(width, height));
        scope.place(&content, 0, 0);
        scope.place(&top, 0, 0);
        scope.place(&bottom, 0, height - bottom.height);
        if fab.width > 0 && fab.height > 0 {
            let margin = density.round_to_px(16.dp());
            scope.place(
                &fab,
                width - fab.width - margin,
                height - bottom.height - fab.height - margin,
            );
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "Scaffold"
    }
}
