//! `Icon` and the icon buttons (`IconButton`, `IconToggleButton`, `FilledIconButton`,
//! `FilledTonalIconButton`, `OutlinedIconButton`), as Compose's, and Material's filled icons
//! (`icons::filled::add()` for `Icons.Filled.Add`).

use compose_foundation::layout::Box;
use compose_foundation::{BorderStroke, CIRCLE_SHAPE};
use compose_runtime::{CompositionLocalProvider, composable};
use compose_ui::{
    Alignment, Color, Dp, DrawModifierNode, DrawScope, ImageVector, Modifier, ModifierNode,
    ModifierNodeElement, MutableInteractionSource, Offset, Paint, Role, Shape, UnitExt,
};

use crate::components::minimum_interactive_component_size;
use crate::theme::{LOCAL_CONTENT_COLOR, MaterialTheme};

/// A vector icon, as Compose's `Icon`: its default size (24 dp), every path drawn in `tint` (by
/// default the content color, as `ColorFilter.tint`), or in its own colors with
/// `Color::UNSPECIFIED`. `content_description` is what accessibility reads; `None` for
/// decoration.
#[composable]
pub fn Icon(
    image_vector: ImageVector,
    content_description: Option<&'static str>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::LOCAL_CONTENT_COLOR.current())] tint: Color,
) {
    let (width, height) = (image_vector.default_width, image_vector.default_height);
    let modifier = match content_description {
        Some(description) => modifier.semantics(false, move |config| {
            config.content_description.push(description.into());
            config.role = Some(Role::Image);
        }),
        None => modifier,
    };
    compose_foundation::layout::Spacer(
        modifier.size_wh(width, height).then_custom(IconElement { image_vector, tint }),
    );
}

struct IconElement {
    image_vector: ImageVector,
    tint: Color,
}

struct IconNode {
    image_vector: ImageVector,
    tint: Color,
}

impl ModifierNodeElement for IconElement {
    fn create(&self) -> std::boxed::Box<dyn ModifierNode> {
        std::boxed::Box::new(IconNode { image_vector: self.image_vector.clone(), tint: self.tint })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<IconNode>() {
            node.image_vector = self.image_vector.clone();
            node.tint = self.tint;
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        use compose_runtime::ComposeArg;
        other.downcast_ref::<IconElement>().is_some_and(|other| {
            other.tint == self.tint && other.image_vector.same(&self.image_vector)
        })
    }
    fn name(&self) -> &'static str {
        "icon"
    }
}

impl ModifierNode for IconNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for IconNode {
    /// The paths, their viewport fitted in the node (its aspect ratio kept) and centered on
    /// whole pixels, as Compose's `Icon` (`ContentScale.Fit`, `Alignment.Center`'s rounding):
    /// Android draws a small path from a mask, unfiltered there (D190).
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let size = scope.size();
        let vector = &self.image_vector;
        let scale = (size.width / vector.viewport_width).min(size.height / vector.viewport_height);
        let offset = Offset::new(
            centered(size.width, vector.viewport_width * scale),
            centered(size.height, vector.viewport_height * scale),
        );
        for (path, fill) in vector.paths.iter() {
            // Tinted as `SrcIn`: the tint, as opaque as the path; untinted, the path's own
            // color (black if it has none, as Material's icons).
            let own = fill.unwrap_or(Color::BLACK);
            let color = if self.tint.is_specified() {
                self.tint
                    .with_alpha(self.tint.alpha() as f32 / 255.0 * (own.alpha() as f32 / 255.0))
            } else {
                own
            };
            scope.draw_path_scaled(path, offset, (scale, scale), Paint::fill(color));
        }
    }
}

/// An icon button's colors, as Compose's `IconButtonColors`. Change some of the defaults with
/// Rust's struct update syntax.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct IconButtonColors {
    pub container_color: Color,
    pub content_color: Color,
    pub disabled_container_color: Color,
    pub disabled_content_color: Color,
}

impl IconButtonColors {
    pub fn container_color(&self, enabled: bool) -> Color {
        if enabled { self.container_color } else { self.disabled_container_color }
    }

    pub fn content_color(&self, enabled: bool) -> Color {
        if enabled { self.content_color } else { self.disabled_content_color }
    }
}

impl compose_runtime::ComposeArg for IconButtonColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// An icon toggle button's colors, as Compose's `IconToggleButtonColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct IconToggleButtonColors {
    pub container_color: Color,
    pub content_color: Color,
    pub disabled_container_color: Color,
    pub disabled_content_color: Color,
    pub checked_container_color: Color,
    pub checked_content_color: Color,
}

impl IconToggleButtonColors {
    pub fn container_color(&self, enabled: bool, checked: bool) -> Color {
        match (enabled, checked) {
            (false, _) => self.disabled_container_color,
            (true, true) => self.checked_container_color,
            (true, false) => self.container_color,
        }
    }

    pub fn content_color(&self, enabled: bool, checked: bool) -> Color {
        match (enabled, checked) {
            (false, _) => self.disabled_content_color,
            (true, true) => self.checked_content_color,
            (true, false) => self.content_color,
        }
    }
}

impl compose_runtime::ComposeArg for IconToggleButtonColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `IconButtonDefaults`.
pub struct IconButtonDefaults;

impl IconButtonDefaults {
    /// A circle, as `filledShape`.
    pub fn filled_shape() -> Shape {
        CIRCLE_SHAPE
    }

    /// A circle, as `outlinedShape`.
    pub fn outlined_shape() -> Shape {
        CIRCLE_SHAPE
    }

    /// No container, the content color, as `iconButtonColors()`.
    pub fn icon_button_colors() -> IconButtonColors {
        let content = LOCAL_CONTENT_COLOR.current();
        IconButtonColors {
            container_color: Color::TRANSPARENT,
            content_color: content,
            disabled_container_color: Color::TRANSPARENT,
            disabled_content_color: content.with_alpha(0.38),
        }
    }

    /// As `iconToggleButtonColors()`: the primary color when checked.
    pub fn icon_toggle_button_colors() -> IconToggleButtonColors {
        let content = LOCAL_CONTENT_COLOR.current();
        IconToggleButtonColors {
            container_color: Color::TRANSPARENT,
            content_color: content,
            disabled_container_color: Color::TRANSPARENT,
            disabled_content_color: content.with_alpha(0.38),
            checked_container_color: Color::TRANSPARENT,
            checked_content_color: MaterialTheme::color_scheme().primary,
        }
    }

    /// The primary container, as `filledIconButtonColors()`.
    pub fn filled_icon_button_colors() -> IconButtonColors {
        let scheme = MaterialTheme::color_scheme();
        IconButtonColors {
            container_color: scheme.primary,
            content_color: scheme.on_primary,
            disabled_container_color: scheme.on_surface.with_alpha(0.12),
            disabled_content_color: scheme.on_surface.with_alpha(0.38),
        }
    }

    /// The secondary container, as `filledTonalIconButtonColors()`.
    pub fn filled_tonal_icon_button_colors() -> IconButtonColors {
        let scheme = MaterialTheme::color_scheme();
        IconButtonColors {
            container_color: scheme.secondary_container,
            content_color: scheme.on_secondary_container,
            disabled_container_color: scheme.on_surface.with_alpha(0.12),
            disabled_content_color: scheme.on_surface.with_alpha(0.38),
        }
    }

    /// No container, the content color, as `outlinedIconButtonColors()`.
    pub fn outlined_icon_button_colors() -> IconButtonColors {
        IconButtonDefaults::icon_button_colors()
    }

    /// A 1 dp outline in the content color (12 % disabled), as `outlinedIconButtonBorder`.
    pub fn outlined_icon_button_border(enabled: bool) -> BorderStroke {
        let content = LOCAL_CONTENT_COLOR.current();
        BorderStroke::new(1.dp(), if enabled { content } else { content.with_alpha(0.12) })
    }
}

/// Where `content` is centered in `space`, as Compose's `Alignment.Center` places it: both
/// sizes rounded to whole pixels, then the half of the difference (`roundToInt`: half up).
fn centered(space: f32, content: f32) -> f32 {
    ((space.round() - content.round()) / 2.0 + 0.5).floor()
}

/// An icon button, as Compose's `IconButton`: a 40 dp circular state layer in a 48 dp touch
/// target around its icon, in the content color.
#[composable]
pub fn IconButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::IconButtonDefaults::icon_button_colors())] colors: IconButtonColors,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    let color = colors.content_color(enabled);
    let mut modifier = minimum_interactive_component_size(modifier)
        .size_wh(40.dp(), 40.dp())
        .clip(CIRCLE_SHAPE)
        .background(colors.container_color(enabled));
    modifier = match interaction_source {
        Some(interaction_source) => modifier.clickable_interaction(
            enabled,
            Some(Role::Button),
            color,
            interaction_source,
            on_click,
        ),
        None => modifier.clickable_indicated(enabled, Some(Role::Button), color, on_click),
    };
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
        Box(
            modifier.clone(),
            Alignment::CENTER,
            false,
            compose_runtime::callback(move |_| content()),
        );
    });
}

/// An icon button that toggles, as Compose's `IconToggleButton`: as `IconButton`, a click
/// calling `on_checked_change(!checked)`; the primary color when checked.
#[composable]
pub fn IconToggleButton(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::IconButtonDefaults::icon_toggle_button_colors())]
    colors: IconToggleButtonColors,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    let color = colors.content_color(enabled, checked);
    let modifier = minimum_interactive_component_size(modifier)
        .size_wh(40.dp(), 40.dp())
        .clip(CIRCLE_SHAPE)
        .background(colors.container_color(enabled, checked))
        .clickable_interaction(
            enabled,
            Some(Role::Checkbox),
            color,
            interaction_source,
            move || on_checked_change(!checked),
        )
        .semantics(false, move |config| config.toggleable_state = Some(checked.into()));
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
        Box(
            modifier.clone(),
            Alignment::CENTER,
            false,
            compose_runtime::callback(move |_| content()),
        );
    });
}

/// The filled and outlined icon buttons' body: a clickable surface of `shape` around a 40 dp
/// box centering the content.
#[allow(clippy::too_many_arguments)]
fn surface_icon_button(
    on_click: compose_runtime::Callback,
    modifier: Modifier,
    enabled: bool,
    shape: Shape,
    colors: IconButtonColors,
    border: Option<BorderStroke>,
    interaction_source: Option<MutableInteractionSource>,
    content: compose_runtime::Callback,
) {
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    let click = crate::components::SurfaceClick {
        on_click,
        enabled,
        role: Some(Role::Button),
        interaction_source,
        selected: None,
        elevations: None,
    };
    crate::components::surface(
        modifier,
        shape,
        colors.container_color(enabled),
        colors.content_color(enabled),
        Dp(0.0),
        Dp(0.0),
        border,
        Some(click),
        compose_runtime::callback(move || {
            let content = content.clone();
            Box(
                Modifier.size(40.dp()),
                Alignment::CENTER,
                false,
                compose_runtime::callback(move |_| content()),
            );
        }),
    );
}

/// An icon button on a filled circle, as Compose's `FilledIconButton`.
#[composable]
pub fn FilledIconButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::IconButtonDefaults::filled_shape())] shape: Shape,
    #[default(crate::IconButtonDefaults::filled_icon_button_colors())] colors: IconButtonColors,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    surface_icon_button(
        on_click,
        modifier,
        enabled,
        shape,
        colors,
        None,
        interaction_source,
        content,
    );
}

/// An icon button on a circle in the secondary container, as Compose's `FilledTonalIconButton`.
#[composable]
pub fn FilledTonalIconButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::IconButtonDefaults::filled_shape())] shape: Shape,
    #[default(crate::IconButtonDefaults::filled_tonal_icon_button_colors())]
    colors: IconButtonColors,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    surface_icon_button(
        on_click,
        modifier,
        enabled,
        shape,
        colors,
        None,
        interaction_source,
        content,
    );
}

/// An icon button in an outlined circle, as Compose's `OutlinedIconButton`; `border` `None` is
/// the default outline, `outlinedIconButtonBorder(enabled)`.
#[composable]
pub fn OutlinedIconButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::IconButtonDefaults::outlined_shape())] shape: Shape,
    #[default(crate::IconButtonDefaults::outlined_icon_button_colors())] colors: IconButtonColors,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    let border = border.or_else(|| Some(IconButtonDefaults::outlined_icon_button_border(enabled)));
    surface_icon_button(
        on_click,
        modifier,
        enabled,
        shape,
        colors,
        border,
        interaction_source,
        content,
    );
}

/// Material's icons, as `Icons`: material-icons-core's filled set (`icons::filled::add()` for
/// `Icons.Filled.Add`, also reachable as `icons::default`). Path data from Google's Material
/// icons, under the Apache License 2.0. Each is parsed once per thread, when first asked for.
pub mod icons {
    /// `Icons.Default`: the filled icons.
    pub use self::filled as default;

    /// `Icons.Filled`.
    pub mod filled {
        use compose_ui::ImageVector;

        // One table of the icons' labels and path data and one accessor parsing each once per
        // thread (D128): an icon's function is a call with its index, so the set costs little
        // to compile.
        macro_rules! icons {
            ($($(#[$doc:meta])* $name:ident = $label:literal, $data:literal;)*) => {
                /// The icons' indexes in `TABLE`.
                #[allow(non_camel_case_types)]
                #[derive(Clone, Copy)]
                enum Index {
                    $($name),*
                }

                const TABLE: &[(&str, &str)] = &[$(($label, $data)),*];

                $(
                    $(#[$doc])*
                    pub fn $name() -> ImageVector {
                        parsed(Index::$name as usize)
                    }
                )*
            };
        }

        /// The icon at `index` of `TABLE`, parsed the first time it's asked for on the thread.
        fn parsed(index: usize) -> ImageVector {
            thread_local! {
                static PARSED: std::cell::RefCell<Vec<Option<ImageVector>>> =
                    std::cell::RefCell::new(vec![None; TABLE.len()]);
            }
            PARSED.with(|parsed| {
                parsed.borrow_mut()[index]
                    .get_or_insert_with(|| {
                        let (label, data) = TABLE[index];
                        ImageVector::icon(label, data)
                    })
                    .clone()
            })
        }

        /// Every icon, in the table's order, for galleries and tests.
        pub fn all() -> Vec<ImageVector> {
            (0..TABLE.len()).map(parsed).collect()
        }

        icons! {
            /// `Icons.Filled.AccountBox`.
            account_box = "Filled.AccountBox", "M3,5v14c0,1.1 0.89,2 2,2h14c1.1,0 2,-0.9 2,-2V5c0,-1.1 -0.9,-2 -2,-2H5c-1.11,0 -2,0.9 -2,2zM15,9c0,1.66 -1.34,3 -3,3s-3,-1.34 -3,-3 1.34,-3 3,-3 3,1.34 3,3zM6,17c0,-2 4,-3.1 6,-3.1s6,1.1 6,3.1v1H6v-1z";
            /// `Icons.Filled.AccountCircle`.
            account_circle = "Filled.AccountCircle", "M12,2C6.48,2 2,6.48 2,12s4.48,10 10,10 10,-4.48 10,-10S17.52,2 12,2zM12,5c1.66,0 3,1.34 3,3s-1.34,3 -3,3 -3,-1.34 -3,-3 1.34,-3 3,-3zM12,19.2c-2.5,0 -4.71,-1.28 -6,-3.22 0.03,-1.99 4,-3.08 6,-3.08 1.99,0 5.97,1.09 6,3.08 -1.29,1.94 -3.5,3.22 -6,3.22z";
            /// `Icons.Filled.Add`: a plus.
            add = "Filled.Add", "M19,13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";
            /// `Icons.Filled.AddCircle`.
            add_circle = "Filled.AddCircle", "M12,2C6.48,2 2,6.48 2,12s4.48,10 10,10 10,-4.48 10,-10S17.52,2 12,2zM17,13h-4v4h-2v-4L7,13v-2h4L11,7h2v4h4v2z";
            /// `Icons.Filled.ArrowBack`.
            arrow_back = "Filled.ArrowBack", "M20,11H7.83l5.59,-5.59L12,4l-8,8 8,8 1.41,-1.41L7.83,13H20v-2z";
            /// `Icons.Filled.ArrowDropDown`.
            arrow_drop_down = "Filled.ArrowDropDown", "M7,10l5,5 5,-5z";
            /// `Icons.Filled.ArrowForward`.
            arrow_forward = "Filled.ArrowForward", "M12,4l-1.41,1.41L16.17,11H4v2h12.17l-5.58,5.59L12,20l8,-8z";
            /// `Icons.Filled.Build`.
            build = "Filled.Build", "M22.7,19l-9.1,-9.1c0.9,-2.3 0.4,-5 -1.5,-6.9 -2,-2 -5,-2.4 -7.4,-1.3L9,6 6,9 1.6,4.7C0.4,7.1 0.9,10.1 2.9,12.1c1.9,1.9 4.6,2.4 6.9,1.5l9.1,9.1c0.4,0.4 1,0.4 1.4,0l2.3,-2.3c0.5,-0.4 0.5,-1.1 0.1,-1.4z";
            /// `Icons.Filled.Call`.
            call = "Filled.Call", "M20.01,15.38c-1.23,0 -2.42,-0.2 -3.53,-0.56 -0.35,-0.12 -0.74,-0.03 -1.01,0.24l-1.57,1.97c-2.83,-1.35 -5.48,-3.9 -6.89,-6.83l1.95,-1.66c0.27,-0.28 0.35,-0.67 0.24,-1.02 -0.37,-1.11 -0.56,-2.3 -0.56,-3.53 0,-0.54 -0.45,-0.99 -0.99,-0.99H4.19C3.65,3 3,3.24 3,3.99 3,13.28 10.73,21 20.01,21c0.71,0 0.99,-0.63 0.99,-1.18v-3.45c0,-0.54 -0.45,-0.99 -0.99,-0.99z";
            /// `Icons.Filled.Check`.
            check = "Filled.Check", "M9,16.17L4.83,12l-1.42,1.41L9,19 21,7l-1.41,-1.41z";
            /// `Icons.Filled.CheckCircle`.
            check_circle = "Filled.CheckCircle", "M12,2C6.48,2 2,6.48 2,12s4.48,10 10,10 10,-4.48 10,-10S17.52,2 12,2zM10,17l-5,-5 1.41,-1.41L10,14.17l7.59,-7.59L19,8l-9,9z";
            /// `Icons.Filled.Clear`.
            clear = "Filled.Clear", "M19,6.41L17.59,5 12,10.59 6.41,5 5,6.41 10.59,12 5,17.59 6.41,19 12,13.41 17.59,19 19,17.59 13.41,12z";
            /// `Icons.Filled.Close`.
            close = "Filled.Close", "M19,6.41L17.59,5 12,10.59 6.41,5 5,6.41 10.59,12 5,17.59 6.41,19 12,13.41 17.59,19 19,17.59 13.41,12z";
            /// `Icons.Filled.Create`.
            create = "Filled.Create", "M3,17.25V21h3.75L17.81,9.94l-3.75,-3.75L3,17.25zM20.71,7.04c0.39,-0.39 0.39,-1.02 0,-1.41l-2.34,-2.34c-0.39,-0.39 -1.02,-0.39 -1.41,0l-1.83,1.83 3.75,3.75 1.83,-1.83z";
            /// `Icons.Filled.DateRange`.
            date_range = "Filled.DateRange", "M9,11L7,11v2h2v-2zM13,11h-2v2h2v-2zM17,11h-2v2h2v-2zM19,4h-1L18,2h-2v2L8,4L8,2L6,2v2L5,4c-1.11,0 -1.99,0.9 -1.99,2L3,20c0,1.1 0.89,2 2,2h14c1.1,0 2,-0.9 2,-2L21,6c0,-1.1 -0.9,-2 -2,-2zM19,20L5,20L5,9h14v11z";
            /// `Icons.Filled.Delete`.
            delete = "Filled.Delete", "M6,19c0,1.1 0.9,2 2,2h8c1.1,0 2,-0.9 2,-2V7H6v12zM19,4h-3.5l-1,-1h-5l-1,1H5v2h14V4z";
            /// `Icons.Filled.Done`.
            done = "Filled.Done", "M9,16.2L4.8,12l-1.4,1.4L9,19 21,7l-1.4,-1.4L9,16.2z";
            /// `Icons.Filled.Edit`.
            edit = "Filled.Edit", "M3,17.25V21h3.75L17.81,9.94l-3.75,-3.75L3,17.25zM20.71,7.04c0.39,-0.39 0.39,-1.02 0,-1.41l-2.34,-2.34c-0.39,-0.39 -1.02,-0.39 -1.41,0l-1.83,1.83 3.75,3.75 1.83,-1.83z";
            /// `Icons.Filled.Email`.
            email = "Filled.Email", "M20,4L4,4c-1.1,0 -1.99,0.9 -1.99,2L2,18c0,1.1 0.9,2 2,2h16c1.1,0 2,-0.9 2,-2L22,6c0,-1.1 -0.9,-2 -2,-2zM20,8l-8,5 -8,-5L4,6l8,5 8,-5v2z";
            /// `Icons.Filled.ExitToApp`.
            exit_to_app = "Filled.ExitToApp", "M10.09,15.59L11.5,17l5,-5 -5,-5 -1.41,1.41L12.67,11H3v2h9.67l-2.58,2.59zM19,3H5c-1.11,0 -2,0.9 -2,2v4h2V5h14v14H5v-4H3v4c0,1.1 0.89,2 2,2h14c1.1,0 2,-0.9 2,-2V5c0,-1.1 -0.9,-2 -2,-2z";
            /// `Icons.Filled.Face`.
            face = "Filled.Face", "M9,11.75c-0.69,0 -1.25,0.56 -1.25,1.25s0.56,1.25 1.25,1.25 1.25,-0.56 1.25,-1.25 -0.56,-1.25 -1.25,-1.25zM15,11.75c-0.69,0 -1.25,0.56 -1.25,1.25s0.56,1.25 1.25,1.25 1.25,-0.56 1.25,-1.25 -0.56,-1.25 -1.25,-1.25zM12,2C6.48,2 2,6.48 2,12s4.48,10 10,10 10,-4.48 10,-10S17.52,2 12,2zM12,20c-4.41,0 -8,-3.59 -8,-8 0,-0.29 0.02,-0.58 0.05,-0.86 2.36,-1.05 4.23,-2.98 5.21,-5.37C11.07,8.33 14.05,10 17.42,10c0.78,0 1.53,-0.09 2.25,-0.26 0.21,0.71 0.33,1.47 0.33,2.26 0,4.41 -3.59,8 -8,8z";
            /// `Icons.Filled.Favorite`.
            favorite = "Filled.Favorite", "M12,21.35l-1.45,-1.32C5.4,15.36 2,12.28 2,8.5 2,5.42 4.42,3 7.5,3c1.74,0 3.41,0.81 4.5,2.09C13.09,3.81 14.76,3 16.5,3 19.58,3 22,5.42 22,8.5c0,3.78 -3.4,6.86 -8.55,11.54L12,21.35z";
            /// `Icons.Filled.FavoriteBorder`.
            favorite_border = "Filled.FavoriteBorder", "M16.5,3c-1.74,0 -3.41,0.81 -4.5,2.09C10.91,3.81 9.24,3 7.5,3 4.42,3 2,5.42 2,8.5c0,3.78 3.4,6.86 8.55,11.54L12,21.35l1.45,-1.32C18.6,15.36 22,12.28 22,8.5 22,5.42 19.58,3 16.5,3zM12.1,18.55l-0.1,0.1 -0.1,-0.1C7.14,14.24 4,11.39 4,8.5 4,6.5 5.5,5 7.5,5c1.54,0 3.04,0.99 3.57,2.36h1.87C13.46,5.99 14.96,5 16.5,5c2,0 3.5,1.5 3.5,3.5 0,2.89 -3.14,5.74 -7.9,10.05z";
            /// `Icons.Filled.Home`.
            home = "Filled.Home", "M10,20v-6h4v6h5v-8h3L12,3 2,12h3v8z";
            /// `Icons.Filled.Info`.
            info = "Filled.Info", "M12,2C6.48,2 2,6.48 2,12s4.48,10 10,10 10,-4.48 10,-10S17.52,2 12,2zM13,17h-2v-6h2v6zM13,9h-2V7h2v2z";
            /// `Icons.Filled.KeyboardArrowDown`.
            keyboard_arrow_down = "Filled.KeyboardArrowDown", "M7.41,8.59L12,13.17l4.59,-4.58L18,10l-6,6 -6,-6 1.41,-1.41z";
            /// `Icons.Filled.KeyboardArrowLeft`.
            keyboard_arrow_left = "Filled.KeyboardArrowLeft", "M15.41,16.59L10.83,12l4.58,-4.59L14,6l-6,6 6,6 1.41,-1.41z";
            /// `Icons.Filled.KeyboardArrowRight`.
            keyboard_arrow_right = "Filled.KeyboardArrowRight", "M8.59,16.59L13.17,12 8.59,7.41 10,6l6,6 -6,6 -1.41,-1.41z";
            /// `Icons.Filled.KeyboardArrowUp`.
            keyboard_arrow_up = "Filled.KeyboardArrowUp", "M7.41,15.41L12,10.83l4.59,4.58L18,14l-6,-6 -6,6z";
            /// `Icons.Filled.List`.
            list = "Filled.List", "M3,13h2v-2H3v2zM3,17h2v-2H3v2zM3,9h2V7H3v2zM7,13h14v-2H7v2zM7,17h14v-2H7v2zM7,7v2h14V7H7z";
            /// `Icons.Filled.LocationOn`.
            location_on = "Filled.LocationOn", "M12,2C8.13,2 5,5.13 5,9c0,5.25 7,13 7,13s7,-7.75 7,-13c0,-3.87 -3.13,-7 -7,-7zM12,11.5c-1.38,0 -2.5,-1.12 -2.5,-2.5s1.12,-2.5 2.5,-2.5 2.5,1.12 2.5,2.5 -1.12,2.5 -2.5,2.5z";
            /// `Icons.Filled.Lock`.
            lock = "Filled.Lock", "M18,8h-1L17,6c0,-2.76 -2.24,-5 -5,-5S7,3.24 7,6v2L6,8c-1.1,0 -2,0.9 -2,2v10c0,1.1 0.9,2 2,2h12c1.1,0 2,-0.9 2,-2L20,10c0,-1.1 -0.9,-2 -2,-2zM12,17c-1.1,0 -2,-0.9 -2,-2s0.9,-2 2,-2 2,0.9 2,2 -0.9,2 -2,2zM15.1,8L8.9,8L8.9,6c0,-1.71 1.39,-3.1 3.1,-3.1 1.71,0 3.1,1.39 3.1,3.1v2z";
            /// `Icons.Filled.MailOutline`.
            mail_outline = "Filled.MailOutline", "M20,4L4,4c-1.1,0 -1.99,0.9 -1.99,2L2,18c0,1.1 0.9,2 2,2h16c1.1,0 2,-0.9 2,-2L22,6c0,-1.1 -0.9,-2 -2,-2zM20,18L4,18L4,8l8,5 8,-5v10zM12,11L4,6h16l-8,5z";
            /// `Icons.Filled.Menu`.
            menu = "Filled.Menu", "M3,18h18v-2H3v2zM3,13h18v-2H3v2zM3,6v2h18V6H3z";
            /// `Icons.Filled.MoreVert`.
            more_vert = "Filled.MoreVert", "M12,8c1.1,0 2,-0.9 2,-2s-0.9,-2 -2,-2 -2,0.9 -2,2 0.9,2 2,2zM12,10c-1.1,0 -2,0.9 -2,2s0.9,2 2,2 2,-0.9 2,-2 -0.9,-2 -2,-2zM12,16c-1.1,0 -2,0.9 -2,2s0.9,2 2,2 2,-0.9 2,-2 -0.9,-2 -2,-2z";
            /// `Icons.Filled.Notifications`.
            notifications = "Filled.Notifications", "M12,22c1.1,0 2,-0.9 2,-2h-4c0,1.1 0.89,2 2,2zM18,16v-5c0,-3.07 -1.64,-5.64 -4.5,-6.32L13.5,4c0,-0.83 -0.67,-1.5 -1.5,-1.5s-1.5,0.67 -1.5,1.5v0.68C7.63,5.36 6,7.92 6,11v5l-2,2v1h16v-1l-2,-2z";
            /// `Icons.Filled.Person`.
            person = "Filled.Person", "M12,12c2.21,0 4,-1.79 4,-4s-1.79,-4 -4,-4 -4,1.79 -4,4 1.79,4 4,4zM12,14c-2.67,0 -8,1.34 -8,4v2h16v-2c0,-2.66 -5.33,-4 -8,-4z";
            /// `Icons.Filled.Phone`.
            phone = "Filled.Phone", "M6.62,10.79c1.44,2.83 3.76,5.14 6.59,6.59l2.2,-2.2c0.27,-0.27 0.67,-0.36 1.02,-0.24 1.12,0.37 2.33,0.57 3.57,0.57 0.55,0 1,0.45 1,1V20c0,0.55 -0.45,1 -1,1 -9.39,0 -17,-7.61 -17,-17 0,-0.55 0.45,-1 1,-1h3.5c0.55,0 1,0.45 1,1 0,1.25 0.2,2.45 0.57,3.57 0.11,0.35 0.03,0.74 -0.25,1.02l-2.2,2.2z";
            /// `Icons.Filled.Place`.
            place = "Filled.Place", "M12,2C8.13,2 5,5.13 5,9c0,5.25 7,13 7,13s7,-7.75 7,-13c0,-3.87 -3.13,-7 -7,-7zM12,11.5c-1.38,0 -2.5,-1.12 -2.5,-2.5s1.12,-2.5 2.5,-2.5 2.5,1.12 2.5,2.5 -1.12,2.5 -2.5,2.5z";
            /// `Icons.Filled.PlayArrow`.
            play_arrow = "Filled.PlayArrow", "M8,5v14l11,-7z";
            /// `Icons.Filled.Refresh`.
            refresh = "Filled.Refresh", "M17.65,6.35C16.2,4.9 14.21,4 12,4c-4.42,0 -7.99,3.58 -7.99,8s3.57,8 7.99,8c3.73,0 6.84,-2.55 7.73,-6h-2.08c-0.82,2.33 -3.04,4 -5.65,4 -3.31,0 -6,-2.69 -6,-6s2.69,-6 6,-6c1.66,0 3.14,0.69 4.22,1.78L13,11h7V4l-2.35,2.35z";
            /// `Icons.Filled.Search`.
            search = "Filled.Search", "M15.5,14h-0.79l-0.28,-0.27C15.41,12.59 16,11.11 16,9.5 16,5.91 13.09,3 9.5,3S3,5.91 3,9.5 5.91,16 9.5,16c1.61,0 3.09,-0.59 4.23,-1.57l0.27,0.28v0.79l5,4.99L20.49,19l-4.99,-5zM9.5,14C7.01,14 5,11.99 5,9.5S7.01,5 9.5,5 14,7.01 14,9.5 11.99,14 9.5,14z";
            /// `Icons.Filled.Send`.
            send = "Filled.Send", "M2.01,21L23,12 2.01,3 2,10l15,2 -15,2z";
            /// `Icons.Filled.Settings`.
            settings = "Filled.Settings", "M19.14,12.94c0.04,-0.3 0.06,-0.61 0.06,-0.94c0,-0.32 -0.02,-0.64 -0.07,-0.94l2.03,-1.58c0.18,-0.14 0.23,-0.41 0.12,-0.61l-1.92,-3.32c-0.12,-0.22 -0.37,-0.29 -0.59,-0.22l-2.39,0.96c-0.5,-0.38 -1.03,-0.7 -1.62,-0.94L14.4,2.81c-0.04,-0.24 -0.24,-0.41 -0.48,-0.41h-3.84c-0.24,0 -0.43,0.17 -0.47,0.41L9.25,5.35C8.66,5.59 8.12,5.92 7.63,6.29L5.24,5.33c-0.22,-0.08 -0.47,0 -0.59,0.22L2.74,8.87C2.62,9.08 2.66,9.34 2.86,9.48l2.03,1.58C4.84,11.36 4.8,11.69 4.8,12s0.02,0.64 0.07,0.94l-2.03,1.58c-0.18,0.14 -0.23,0.41 -0.12,0.61l1.92,3.32c0.12,0.22 0.37,0.29 0.59,0.22l2.39,-0.96c0.5,0.38 1.03,0.7 1.62,0.94l0.36,2.54c0.05,0.24 0.24,0.41 0.48,0.41h3.84c0.24,0 0.44,-0.17 0.47,-0.41l0.36,-2.54c0.59,-0.24 1.13,-0.56 1.62,-0.94l2.39,0.96c0.22,0.08 0.47,0 0.59,-0.22l1.92,-3.32c0.12,-0.22 0.07,-0.47 -0.12,-0.61L19.14,12.94zM12,15.6c-1.98,0 -3.6,-1.62 -3.6,-3.6s1.62,-3.6 3.6,-3.6s3.6,1.62 3.6,3.6S13.98,15.6 12,15.6z";
            /// `Icons.Filled.Share`.
            share = "Filled.Share", "M18,16.08c-0.76,0 -1.44,0.3 -1.96,0.77L8.91,12.7c0.05,-0.23 0.09,-0.46 0.09,-0.7s-0.04,-0.47 -0.09,-0.7l7.05,-4.11c0.54,0.5 1.25,0.81 2.04,0.81 1.66,0 3,-1.34 3,-3s-1.34,-3 -3,-3 -3,1.34 -3,3c0,0.24 0.04,0.47 0.09,0.7L8.04,9.81C7.5,9.31 6.79,9 6,9c-1.66,0 -3,1.34 -3,3s1.34,3 3,3c0.79,0 1.5,-0.31 2.04,-0.81l7.12,4.16c-0.05,0.21 -0.08,0.43 -0.08,0.65 0,1.61 1.31,2.92 2.92,2.92 1.61,0 2.92,-1.31 2.92,-2.92s-1.31,-2.92 -2.92,-2.92z";
            /// `Icons.Filled.ShoppingCart`.
            shopping_cart = "Filled.ShoppingCart", "M7,18c-1.1,0 -1.99,0.9 -1.99,2S5.9,22 7,22s2,-0.9 2,-2 -0.9,-2 -2,-2zM1,2v2h2l3.6,7.59 -1.35,2.45c-0.16,0.28 -0.25,0.61 -0.25,0.96 0,1.1 0.9,2 2,2h12v-2L7.42,15c-0.14,0 -0.25,-0.11 -0.25,-0.25l0.03,-0.12 0.9,-1.63h7.45c0.75,0 1.41,-0.41 1.75,-1.03l3.58,-6.49C21.25,4.82 20.76,4 20,4H5.21l-0.94,-2H1zM17,18c-1.1,0 -1.99,0.9 -1.99,2s0.89,2 1.99,2 2,-0.9 2,-2 -0.9,-2 -2,-2z";
            /// `Icons.Filled.Star`.
            star = "Filled.Star", "M12,17.27L18.18,21l-1.64,-7.03L22,9.24l-7.19,-0.61L12,2 9.19,8.63 2,9.24l5.46,4.73L5.82,21z";
            /// `Icons.Filled.ThumbUp`.
            thumb_up = "Filled.ThumbUp", "M1,21h4L5,9L1,9v12zM23,10c0,-1.1 -0.9,-2 -2,-2h-6.31l0.95,-4.57 0.03,-0.32c0,-0.41 -0.17,-0.79 -0.44,-1.06L14.17,1 7.59,7.59C7.22,7.95 7,8.45 7,9v10c0,1.1 0.9,2 2,2h9c0.83,0 1.54,-0.5 1.84,-1.22l3.02,-7.05c0.09,-0.23 0.14,-0.47 0.14,-0.73v-2z";
            /// `Icons.Filled.Warning`.
            warning = "Filled.Warning", "M1,21h22L12,2 1,21zM13,18h-2v-2h2v2zM13,14h-2v-4h2v4z";
        }
    }
}
