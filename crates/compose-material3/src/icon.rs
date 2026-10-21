//! `Icon`, `IconButton` and `FloatingActionButton`, as Compose's, and a set of Material's filled
//! icons (`icons::filled::add()` for `Icons.Filled.Add`).

use compose_foundation::CIRCLE_SHAPE;
use compose_foundation::layout::Box;
use compose_runtime::{CompositionLocalProvider, composable};
use compose_ui::{
    Alignment, Color, DrawModifierNode, DrawScope, ImageVector, Modifier, ModifierNode,
    ModifierNodeElement, Offset, Paint, Role, Shape, UnitExt,
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
    /// The paths, their viewport fitted in the node (its aspect ratio kept) and centered, as
    /// Compose's `Icon` (`ContentScale.Fit`).
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let size = scope.size();
        let vector = &self.image_vector;
        let scale = (size.width / vector.viewport_width).min(size.height / vector.viewport_height);
        let offset = Offset::new(
            (size.width - vector.viewport_width * scale) / 2.0,
            (size.height - vector.viewport_height * scale) / 2.0,
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

/// An icon button, as Compose's `IconButton`: a 40 dp circular state layer in a 48 dp touch
/// target around its icon, in the content color.
#[composable]
pub fn IconButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    content: impl Fn() + 'static,
) {
    let content_color = LOCAL_CONTENT_COLOR.current();
    let color = if enabled { content_color } else { content_color.with_alpha(0.38) };
    let modifier = minimum_interactive_component_size(modifier)
        .size_wh(40.dp(), 40.dp())
        .clip(CIRCLE_SHAPE)
        .clickable_indicated(enabled, Some(Role::Button), color, on_click);
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
        Box(modifier, Alignment::CENTER, false, compose_runtime::callback(move |_| content()));
    });
}

/// A floating action button, as Compose's `FloatingActionButton`: 56 dp, the large shape, in the
/// primary container's colors.
#[composable]
pub fn FloatingActionButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::MaterialTheme::shapes().large)] shape: Shape,
    #[default(crate::MaterialTheme::color_scheme().primary_container)] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    content: impl Fn() + 'static,
) {
    // By default the content color for the container, as `contentColorFor`.
    let content_color =
        content_color.or(MaterialTheme::color_scheme().content_color_for(container_color));
    // At least 56 dp, growing with its content, as Compose's `defaultMinSize`.
    let modifier = modifier
        .size_in(compose_ui::SizeSpec::bounds(
            56.dp(),
            56.dp(),
            compose_ui::Dp::UNSPECIFIED,
            compose_ui::Dp::UNSPECIFIED,
        ))
        // Its 6 dp shadow, as `FloatingActionButtonDefaults.elevation()`.
        .shadow(6.dp(), shape, false)
        .background_shape(container_color, shape)
        .clip(shape)
        .clickable_indicated(true, Some(Role::Button), content_color, on_click);
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(content_color), || {
        Box(modifier, Alignment::CENTER, false, compose_runtime::callback(move |_| content()));
    });
}

/// Material's icons, as `Icons` (material-icons-core and a few from extended). Path data from
/// Google's Material icons, under the Apache License 2.0.
pub mod icons {
    /// `Icons.Filled`.
    pub mod filled {
        use compose_ui::ImageVector;

        macro_rules! icons {
            ($($(#[$doc:meta])* $name:ident = $label:literal, $data:literal;)*) => {
                $(
                    $(#[$doc])*
                    pub fn $name() -> ImageVector {
                        thread_local! {
                            static ICON: ImageVector = ImageVector::icon($label, $data);
                        }
                        ICON.with(Clone::clone)
                    }
                )*
            };
        }

        icons! {
            /// `Icons.Filled.Add`: a plus.
            add = "Filled.Add", "M19,13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";
            /// `Icons.Filled.ArrowBack`.
            arrow_back = "Filled.ArrowBack", "M20,11H7.83l5.59,-5.59L12,4l-8,8 8,8 1.41,-1.41L7.83,13H20v-2z";
            /// `Icons.Filled.Check`.
            check = "Filled.Check", "M9,16.17L4.83,12l-1.42,1.41L9,19 21,7l-1.41,-1.41z";
            /// `Icons.Filled.Close`.
            close = "Filled.Close", "M19,6.41L17.59,5 12,10.59 6.41,5 5,6.41 10.59,12 5,17.59 6.41,19 12,13.41 17.59,19 19,17.59 13.41,12z";
            /// `Icons.Filled.Delete`.
            delete = "Filled.Delete", "M6,19c0,1.1 0.9,2 2,2h8c1.1,0 2,-0.9 2,-2V7H6v12zM19,4h-3.5l-1,-1h-5l-1,1H5v2h14V4z";
            /// `Icons.Filled.Edit`.
            edit = "Filled.Edit", "M3,17.25V21h3.75L17.81,9.94l-3.75,-3.75L3,17.25zM20.71,7.04c0.39,-0.39 0.39,-1.02 0,-1.41l-2.34,-2.34c-0.39,-0.39 -1.02,-0.39 -1.41,0l-1.83,1.83 3.75,3.75 1.83,-1.83z";
            /// `Icons.Filled.Favorite`.
            favorite = "Filled.Favorite", "M12,21.35l-1.45,-1.32C5.4,15.36 2,12.28 2,8.5 2,5.42 4.42,3 7.5,3c1.74,0 3.41,0.81 4.5,2.09C13.09,3.81 14.76,3 16.5,3 19.58,3 22,5.42 22,8.5c0,3.78 -3.4,6.86 -8.55,11.54L12,21.35z";
            /// `Icons.Filled.Home`.
            home = "Filled.Home", "M10,20v-6h4v6h5v-8h3L12,3 2,12h3v8z";
            /// `Icons.Filled.Info`.
            info = "Filled.Info", "M12,2C6.48,2 2,6.48 2,12s4.48,10 10,10 10,-4.48 10,-10S17.52,2 12,2zM13,17h-2v-6h2v6zM13,9h-2V7h2v2z";
            /// `Icons.Filled.Menu`.
            menu = "Filled.Menu", "M3,18h18v-2H3v2zM3,13h18v-2H3v2zM3,6v2h18V6H3z";
            /// `Icons.Filled.MoreVert`.
            more_vert = "Filled.MoreVert", "M12,8c1.1,0 2,-0.9 2,-2s-0.9,-2 -2,-2 -2,0.9 -2,2 0.9,2 2,2zM12,10c-1.1,0 -2,0.9 -2,2s0.9,2 2,2 2,-0.9 2,-2 -0.9,-2 -2,-2zM12,16c-1.1,0 -2,0.9 -2,2s0.9,2 2,2 2,-0.9 2,-2 -0.9,-2 -2,-2z";
            /// `Icons.Filled.Person`.
            person = "Filled.Person", "M12,12c2.21,0 4,-1.79 4,-4s-1.79,-4 -4,-4 -4,1.79 -4,4 1.79,4 4,4zM12,14c-2.67,0 -8,1.34 -8,4v2h16v-2c0,-2.66 -5.33,-4 -8,-4z";
            /// `Icons.Filled.Search`.
            search = "Filled.Search", "M15.5,14h-0.79l-0.28,-0.27C15.41,12.59 16,11.11 16,9.5 16,5.91 13.09,3 9.5,3S3,5.91 3,9.5 5.91,16 9.5,16c1.61,0 3.09,-0.59 4.23,-1.57l0.27,0.28v0.79l5,4.99L20.49,19l-4.99,-5zM9.5,14C7.01,14 5,11.99 5,9.5S7.01,5 9.5,5 14,7.01 14,9.5 11.99,14 9.5,14z";
            /// `Icons.Filled.Star`.
            star = "Filled.Star", "M12,17.27L18.18,21l-1.64,-7.03L22,9.24l-7.19,-0.61L12,2 9.19,8.63 2,9.24l5.46,4.73L5.82,21z";
        }
    }
}
