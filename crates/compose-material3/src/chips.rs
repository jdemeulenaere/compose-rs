//! Chips and badges, as Compose's Material 3: `AssistChip`, `FilterChip`, `SuggestionChip`,
//! `Badge` and `BadgedBox`.

use std::rc::Rc;

use compose_foundation::BorderStroke;
use compose_foundation::layout::{Arrangement, Box, Row};
use compose_runtime::{CompositionLocalProvider, composable};
use compose_ui::{
    Alignment, Color, Constraints, Dp, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier, Role, Shape, SizeSpec, UnitExt,
};

use crate::components::minimum_interactive_component_size;
use crate::icon::{Icon, icons};
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

/// A chip's look, as resolved for its state.
struct ChipLook {
    container: Color,
    label: Color,
    icon: Color,
    border: Option<BorderStroke>,
}

/// The chip all kinds share, as Compose's `Chip`/`SelectableChip`: 32 dp high on the small
/// shape, its label (label-large) 16 dp in (8 dp beside an 18 dp icon), clickable in a 48 dp
/// target.
#[allow(clippy::too_many_arguments)]
fn chip(
    on_click: compose_runtime::Callback,
    modifier: Modifier,
    enabled: bool,
    label: compose_runtime::Callback,
    leading_icon: Option<compose_runtime::Callback>,
    trailing_icon: Option<compose_runtime::Callback>,
    look: ChipLook,
    role: Role,
    selected: Option<bool>,
) {
    let shape = MaterialTheme::shapes().small;
    let typography = MaterialTheme::typography();
    let mut surface = minimum_interactive_component_size(modifier)
        .size_in(SizeSpec::bounds(Dp::UNSPECIFIED, 32.dp(), Dp::UNSPECIFIED, Dp::UNSPECIFIED))
        .background_shape(look.container, shape)
        .clip(shape);
    if let Some(border) = look.border {
        surface = surface.border(border.width, border.color, shape);
    }
    surface = surface.clickable_indicated(enabled, Some(role), look.label, on_click);
    if let Some(selected) = selected {
        surface = surface.semantics(false, move |config| config.selected = Some(selected));
    }
    let start = if leading_icon.is_some() { 8.dp() } else { 16.dp() };
    let end = if trailing_icon.is_some() { 8.dp() } else { 16.dp() };
    CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(typography.label_large), || {
        let (label, leading, trailing) =
            (label.clone(), leading_icon.clone(), trailing_icon.clone());
        Row(
            surface.clone().padding_sides(start, 0.dp(), end, 0.dp()),
            Arrangement::spaced_by(8.dp()),
            Alignment::CENTER_VERTICALLY,
            compose_runtime::callback(move |_| {
                if let Some(leading) = leading.clone() {
                    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(look.icon), || {
                        Box(
                            Modifier.size(18.dp()),
                            Alignment::CENTER,
                            false,
                            compose_runtime::callback({
                                let leading = leading.clone();
                                move |_| leading()
                            }),
                        );
                    });
                }
                CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(look.label), &*label);
                if let Some(trailing) = trailing.clone() {
                    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(look.icon), || {
                        Box(
                            Modifier.size(18.dp()),
                            Alignment::CENTER,
                            false,
                            compose_runtime::callback({
                                let trailing = trailing.clone();
                                move |_| trailing()
                            }),
                        );
                    });
                }
            }),
        );
    });
}

/// A flat chip's look: an outline, no fill (dimmed when disabled).
fn outlined_look(enabled: bool, label: Color, icon: Color) -> ChipLook {
    let scheme = MaterialTheme::color_scheme();
    let dim = scheme.on_surface.with_alpha(0.38);
    ChipLook {
        container: Color::TRANSPARENT,
        label: if enabled { label } else { dim },
        icon: if enabled { icon } else { dim },
        border: Some(BorderStroke::new(
            Dp(1.0),
            if enabled { scheme.outline } else { scheme.on_surface.with_alpha(0.12) },
        )),
    }
}

/// A chip suggesting an action, as Compose's `AssistChip`.
#[composable]
#[allow(non_snake_case)]
pub fn AssistChip(
    on_click: impl Fn() + 'static,
    label: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
) {
    let scheme = MaterialTheme::color_scheme();
    chip(
        on_click,
        modifier,
        enabled,
        label,
        leading_icon,
        trailing_icon,
        outlined_look(enabled, scheme.on_surface, scheme.primary),
        Role::Button,
        None,
    );
}

/// A chip offering a suggestion, as Compose's `SuggestionChip`.
#[composable]
#[allow(non_snake_case)]
pub fn SuggestionChip(
    on_click: impl Fn() + 'static,
    label: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] icon: Option<impl Fn() + 'static>,
) {
    let scheme = MaterialTheme::color_scheme();
    chip(
        on_click,
        modifier,
        enabled,
        label,
        icon,
        None,
        outlined_look(enabled, scheme.on_surface_variant, scheme.primary),
        Role::Button,
        None,
    );
}

/// A chip that's on or off, as Compose's `FilterChip`: selected, in the secondary container
/// without an outline (and, without a leading icon, a check mark leading).
#[composable]
#[allow(non_snake_case)]
pub fn FilterChip(
    selected: bool,
    on_click: impl Fn() + 'static,
    label: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
) {
    let scheme = MaterialTheme::color_scheme();
    let look = if selected {
        let dim = scheme.on_surface.with_alpha(0.38);
        ChipLook {
            container: if enabled {
                scheme.secondary_container
            } else {
                scheme.on_surface.with_alpha(0.12)
            },
            label: if enabled { scheme.on_secondary_container } else { dim },
            icon: if enabled { scheme.on_secondary_container } else { dim },
            border: None,
        }
    } else {
        outlined_look(enabled, scheme.on_surface_variant, scheme.primary)
    };
    // As Compose's sample: a check mark when selected and given no icon of its own.
    let leading = leading_icon.or_else(|| {
        selected.then(|| {
            compose_runtime::callback(|| {
                Icon(
                    icons::filled::check(),
                    None,
                    Modifier.size(18.dp()),
                    LOCAL_CONTENT_COLOR.current(),
                )
            })
        })
    });
    chip(
        on_click,
        modifier,
        enabled,
        label,
        leading,
        trailing_icon,
        look,
        Role::Checkbox,
        Some(selected),
    );
}

/// A badge, as Compose's `Badge`: a 6 dp dot in the error color, or, with content (a count),
/// a 16 dp high pill around it in the small label style.
#[composable]
#[allow(non_snake_case)]
pub fn Badge(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::MaterialTheme::color_scheme().error)] container_color: Color,
    #[default(crate::MaterialTheme::color_scheme().on_error)] content_color: Color,
    #[default(None)] content: Option<impl Fn() + 'static>,
) {
    let typography = MaterialTheme::typography();
    let pill = Shape::Rounded([compose_ui::CornerSize::Percent(50.0); 4]);
    let size = if content.is_some() { 16.dp() } else { 6.dp() };
    let modifier = modifier
        .size_in(SizeSpec::bounds(size, size, Dp::UNSPECIFIED, Dp::UNSPECIFIED))
        .background_shape(container_color, pill)
        .clip(pill);
    match content {
        Some(content) => {
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(content_color), || {
                CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(typography.label_small), || {
                    let content = content.clone();
                    Box(
                        modifier.clone().padding_horizontal_vertical(4.dp(), 0.dp()),
                        Alignment::CENTER,
                        false,
                        compose_runtime::callback(move |_| content()),
                    );
                });
            });
        }
        None => Box(modifier, Alignment::CENTER, false, compose_runtime::callback(|_| {})),
    }
}

/// `content` (an icon) with `badge` over its top end, as Compose's `BadgedBox`: the badge's
/// vertical center on the content's top, its start 6 dp before the content's end (12 dp for a
/// badge with a count).
#[composable]
#[allow(non_snake_case)]
pub fn BadgedBox(
    badge: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    content: impl Fn() + 'static,
) {
    let policy = compose_runtime::remember(|| Rc::new(BadgedLayout) as Rc<dyn MeasurePolicy>);
    let (badge, content) = (badge.clone(), content.clone());
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            content();
            badge();
        }),
    );
}

/// The content, then the badge over its top end.
struct BadgedLayout;

impl MeasurePolicy for BadgedLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let [content, badge] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let loose = constraints.loosen();
        let content = scope.measure(*content, loose);
        let badge = scope.measure(*badge, loose);
        let density = scope.density();
        let dot = badge.height <= density.round_to_px(6.dp());
        let inset = density.round_to_px(if dot { 6.dp() } else { 12.dp() });
        scope.place_relative(&content, 0, 0);
        scope.place_relative(&badge, content.width - inset, -badge.height / 2);
        scope.layout(content.width, content.height)
    }

    fn debug_name(&self) -> &'static str {
        "BadgedBox"
    }
}
