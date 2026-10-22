//! `ListItem`, as Compose's Material 3: a row of a list, with optional overline, supporting
//! text, and leading and trailing content.

use compose_foundation::layout::{Arrangement, Box, Column, Row, RowScope};
use compose_runtime::{CompositionLocalProvider, composable};
use compose_ui::{Alignment, Color, Dp, Modifier, SizeSpec, UnitExt};

use crate::components::Surface;
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

/// A list item's colors, as Compose's `ListItemColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ListItemColors {
    pub container_color: Color,
    pub headline_color: Color,
    pub leading_icon_color: Color,
    pub overline_color: Color,
    pub supporting_text_color: Color,
    pub trailing_icon_color: Color,
}

impl compose_runtime::ComposeArg for ListItemColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `ListItemDefaults`.
pub struct ListItemDefaults;

impl ListItemDefaults {
    /// From the theme, as `ListItemDefaults.colors()`.
    pub fn colors() -> ListItemColors {
        let scheme = MaterialTheme::color_scheme();
        ListItemColors {
            container_color: scheme.surface,
            headline_color: scheme.on_surface,
            leading_icon_color: scheme.on_surface_variant,
            overline_color: scheme.on_surface_variant,
            supporting_text_color: scheme.on_surface_variant,
            trailing_icon_color: scheme.on_surface_variant,
        }
    }
}

/// A row of a list, as Compose's `ListItem`: its headline in the body-large style, an optional
/// overline (label-small) above and supporting text (body-medium) below, and optional leading
/// and trailing content (an icon, a checkbox, a label), 16 dp from the start, 24 dp from the
/// end, at least 56, 72 or 88 dp high for one, two or three lines.
#[composable]
#[allow(non_snake_case)]
pub fn ListItem(
    headline_content: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] overline_content: Option<impl Fn() + 'static>,
    #[default(None)] supporting_content: Option<impl Fn() + 'static>,
    #[default(None)] leading_content: Option<impl Fn() + 'static>,
    #[default(None)] trailing_content: Option<impl Fn() + 'static>,
    #[default(crate::ListItemDefaults::colors())] colors: ListItemColors,
    #[default(crate::Dp(0.0))] tonal_elevation: Dp,
    #[default(crate::Dp(0.0))] shadow_elevation: Dp,
) {
    let lines =
        1 + usize::from(overline_content.is_some()) + usize::from(supporting_content.is_some());
    let (min_height, vertical) = match lines {
        1 => (56.dp(), 8.dp()),
        2 => (72.dp(), 8.dp()),
        _ => (88.dp(), 12.dp()),
    };
    // Three lines align at the top, as Compose's.
    let alignment = if lines >= 3 { Alignment::TOP } else { Alignment::CENTER_VERTICALLY };
    let typography = MaterialTheme::typography();
    // One node for the item, its texts merged, as Compose's.
    Surface(
        modifier.semantics_with(true, |_| {}),
        compose_ui::RECTANGLE_SHAPE,
        colors.container_color,
        Color::UNSPECIFIED,
        tonal_elevation,
        shadow_elevation,
        None,
        compose_runtime::callback(move || {
            let (headline, overline, supporting, leading, trailing) = (
                headline_content.clone(),
                overline_content.clone(),
                supporting_content.clone(),
                leading_content.clone(),
                trailing_content.clone(),
            );
            Row(
                Modifier
                    .fill_max_width()
                    .size_in(SizeSpec::bounds(
                        Dp::UNSPECIFIED,
                        min_height,
                        Dp::UNSPECIFIED,
                        Dp::UNSPECIFIED,
                    ))
                    .padding((16.dp(), vertical, 24.dp(), vertical)),
                Arrangement::START,
                alignment,
                compose_runtime::callback(move |row: RowScope| {
                    if let Some(leading) = leading.clone() {
                        CompositionLocalProvider(
                            LOCAL_CONTENT_COLOR.provides(colors.leading_icon_color),
                            || {
                                Box(
                                    Modifier.padding((0.dp(), 0.dp(), 16.dp(), 0.dp())),
                                    Alignment::CENTER,
                                    false,
                                    compose_runtime::callback({
                                        let leading = leading.clone();
                                        move |_| leading()
                                    }),
                                );
                            },
                        );
                    }
                    let (headline, overline, supporting) =
                        (headline.clone(), overline.clone(), supporting.clone());
                    Column(
                        Modifier.weight(row, 1.0),
                        Arrangement::TOP,
                        Alignment::START,
                        compose_runtime::callback(move |_| {
                            if let Some(overline) = overline.clone() {
                                styled(colors.overline_color, typography.label_small, &overline);
                            }
                            styled(colors.headline_color, typography.body_large, &headline);
                            if let Some(supporting) = supporting.clone() {
                                styled(
                                    colors.supporting_text_color,
                                    typography.body_medium,
                                    &supporting,
                                );
                            }
                        }),
                    );
                    if let Some(trailing) = trailing.clone() {
                        CompositionLocalProvider(
                            LOCAL_CONTENT_COLOR.provides(colors.trailing_icon_color),
                            || {
                                CompositionLocalProvider(
                                    LOCAL_TEXT_STYLE.provides(typography.label_small),
                                    || {
                                        Box(
                                            Modifier.padding((16.dp(), 0.dp(), 0.dp(), 0.dp())),
                                            Alignment::CENTER,
                                            false,
                                            compose_runtime::callback({
                                                let trailing = trailing.clone();
                                                move |_| trailing()
                                            }),
                                        );
                                    },
                                );
                            },
                        );
                    }
                }),
            );
        }),
    );
}

/// `content` in `color` and `style`.
fn styled(color: Color, style: compose_ui::TextStyle, content: &compose_runtime::Callback) {
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
        CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(style), &**content);
    });
}
