//! The split button, as Compose's Material 3: `SplitButtonLayout` pairing a leading button with a
//! trailing one (often a menu's), their inner corners squarer than their outer ones and moving
//! when pressed or checked, with `SplitButtonDefaults` and its buttons of each kind, and
//! `SplitButtonShapes`.

use compose_foundation::BorderStroke;
use compose_foundation::layout::{Arrangement, Box, PaddingValues, Row, RowScope};
use compose_runtime::{CompositionLocalProvider, composable};
use compose_ui::{
    Alignment, Constraints, CornerSize, Dp, Measurable, MeasureResult, MeasureScope, Modifier,
    MutableInteractionSource, Paint, Role, Shape, Size,
};

use crate::button::{ButtonColors, ButtonDefaults, ButtonElevation};
use crate::components::{
    Elevations, LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE, SurfaceClick,
    minimum_interactive_component_size, remembered_interaction_source, surface,
};
use crate::theme::{MaterialTheme, provide_content_color_text_style};

/// A split button's shapes at rest, pressed and checked, as Compose's `SplitButtonShapes`: `None`
/// keeps the resting one. Its corners move between them if they all have corners.
#[derive(Clone, PartialEq, Debug)]
pub struct SplitButtonShapes {
    pub shape: Shape,
    pub pressed_shape: Option<Shape>,
    pub checked_shape: Option<Shape>,
}

impl compose_runtime::ComposeArg for SplitButtonShapes {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl SplitButtonShapes {
    /// As `shapeByInteraction`: pressed, then checked, on the default effects spec.
    fn for_state(&self, pressed: bool, checked: bool) -> Shape {
        let target = match (pressed, checked) {
            (true, _) => self.pressed_shape.as_ref(),
            (false, true) => self.checked_shape.as_ref(),
            (false, false) => None,
        };
        let target = target.unwrap_or(&self.shape).clone();
        let corner_based = self.shape.corners().is_some()
            && [&self.pressed_shape, &self.checked_shape]
                .iter()
                .all(|shape| shape.as_ref().is_none_or(|shape| shape.corners().is_some()));
        if corner_based {
            crate::animated_shape::remember_animated_shape(
                target,
                MaterialTheme::motion_scheme().default_effects_spec(),
            )
        } else {
            target
        }
    }
}

/// As Compose's `SplitButtonDefaults`.
pub struct SplitButtonDefaults {}

const fn padding(start: f32, end: f32) -> PaddingValues {
    PaddingValues::new(Dp(start), Dp(0.0), Dp(end), Dp(0.0))
}

impl SplitButtonDefaults {
    /// As `LeadingIconSize`.
    pub const LEADING_ICON_SIZE: Dp = Dp(20.0);
    /// As `TrailingIconSize`.
    pub const TRAILING_ICON_SIZE: Dp = Dp(22.0);
    /// Between the two buttons, as `Spacing`.
    pub const SPACING: Dp = Dp(2.0);

    /// The inner corners at each size, as `ExtraSmallInnerCornerSize` and the others.
    pub const EXTRA_SMALL_INNER_CORNER_SIZE: CornerSize = CornerSize::Dp(Dp(4.0));
    pub const SMALL_INNER_CORNER_SIZE: CornerSize = CornerSize::Dp(Dp(4.0));
    pub const MEDIUM_INNER_CORNER_SIZE: CornerSize = CornerSize::Dp(Dp(4.0));
    pub const LARGE_INNER_CORNER_SIZE: CornerSize = CornerSize::Dp(Dp(8.0));
    pub const EXTRA_LARGE_INNER_CORNER_SIZE: CornerSize = CornerSize::Dp(Dp(12.0));
    /// Pressed, as `ExtraSmallInnerCornerSizePressed` and the others.
    pub const EXTRA_SMALL_INNER_CORNER_SIZE_PRESSED: CornerSize = CornerSize::Dp(Dp(8.0));
    pub const SMALL_INNER_CORNER_SIZE_PRESSED: CornerSize = CornerSize::Dp(Dp(12.0));
    pub const MEDIUM_INNER_CORNER_SIZE_PRESSED: CornerSize = CornerSize::Dp(Dp(12.0));
    pub const LARGE_INNER_CORNER_SIZE_PRESSED: CornerSize = CornerSize::Dp(Dp(20.0));
    pub const EXTRA_LARGE_INNER_CORNER_SIZE_PRESSED: CornerSize = CornerSize::Dp(Dp(20.0));
    /// The outer corners, round, as `OuterCornerSize`.
    pub const OUTER_CORNER_SIZE: CornerSize = CornerSize::Percent(50.0);

    /// The leading button's padding at each size, as `ExtraSmallLeadingButtonContentPadding` and
    /// the others.
    pub const EXTRA_SMALL_LEADING_BUTTON_CONTENT_PADDING: PaddingValues = padding(12.0, 10.0);
    pub const SMALL_LEADING_BUTTON_CONTENT_PADDING: PaddingValues = padding(16.0, 12.0);
    pub const MEDIUM_LEADING_BUTTON_CONTENT_PADDING: PaddingValues = padding(24.0, 24.0);
    pub const LARGE_LEADING_BUTTON_CONTENT_PADDING: PaddingValues = padding(48.0, 48.0);
    pub const EXTRA_LARGE_LEADING_BUTTON_CONTENT_PADDING: PaddingValues = padding(64.0, 64.0);
    /// The trailing button's, as `ExtraSmallTrailingButtonContentPadding` and the others.
    pub const EXTRA_SMALL_TRAILING_BUTTON_CONTENT_PADDING: PaddingValues = padding(13.0, 13.0);
    pub const SMALL_TRAILING_BUTTON_CONTENT_PADDING: PaddingValues = padding(13.0, 13.0);
    pub const MEDIUM_TRAILING_BUTTON_CONTENT_PADDING: PaddingValues = padding(15.0, 15.0);
    pub const LARGE_TRAILING_BUTTON_CONTENT_PADDING: PaddingValues = padding(29.0, 29.0);
    pub const EXTRA_LARGE_TRAILING_BUTTON_CONTENT_PADDING: PaddingValues = padding(43.0, 43.0);
    /// The trailing button's icon, as `ExtraSmallTrailingButtonIconSize` and the others.
    pub const EXTRA_SMALL_TRAILING_BUTTON_ICON_SIZE: Dp = Dp(22.0);
    pub const SMALL_TRAILING_BUTTON_ICON_SIZE: Dp = Dp(22.0);
    pub const MEDIUM_TRAILING_BUTTON_ICON_SIZE: Dp = Dp(26.0);
    pub const LARGE_TRAILING_BUTTON_ICON_SIZE: Dp = Dp(38.0);
    pub const EXTRA_LARGE_TRAILING_BUTTON_ICON_SIZE: Dp = Dp(50.0);
    /// The heights, as `ExtraSmallContainerHeight` and the others.
    pub const EXTRA_SMALL_CONTAINER_HEIGHT: Dp = Dp(32.0);
    pub const SMALL_CONTAINER_HEIGHT: Dp = Dp(40.0);
    pub const MEDIUM_CONTAINER_HEIGHT: Dp = Dp(56.0);
    pub const LARGE_CONTAINER_HEIGHT: Dp = Dp(96.0);
    pub const EXTRA_LARGE_CONTAINER_HEIGHT: Dp = Dp(136.0);

    const BUTTON_MIN_WIDTH: Dp = Dp(48.0);

    /// Which size `height` is closest to, 0 extra small to 4 extra large, as the shapes' `when`.
    fn nearest_size(height: Dp) -> usize {
        let heights = [
            ButtonDefaults::EXTRA_SMALL_CONTAINER_HEIGHT,
            ButtonDefaults::MIN_HEIGHT,
            ButtonDefaults::MEDIUM_CONTAINER_HEIGHT,
            ButtonDefaults::LARGE_CONTAINER_HEIGHT,
            ButtonDefaults::EXTRA_LARGE_CONTAINER_HEIGHT,
        ];
        heights.windows(2).take_while(|pair| height.0 > (pair[0].0 + pair[1].0) / 2.0).count()
    }

    /// Which size `height` has reached, as the paddings' and icons' `when`.
    fn reached_size(height: Dp) -> usize {
        [
            Self::SMALL_CONTAINER_HEIGHT,
            Self::MEDIUM_CONTAINER_HEIGHT,
            Self::LARGE_CONTAINER_HEIGHT,
            Self::EXTRA_LARGE_CONTAINER_HEIGHT,
        ]
        .iter()
        .take_while(|it| height.0 >= it.0)
        .count()
    }

    fn inner_corners(button_height: Dp) -> (CornerSize, CornerSize) {
        [
            (Self::EXTRA_SMALL_INNER_CORNER_SIZE, Self::EXTRA_SMALL_INNER_CORNER_SIZE_PRESSED),
            (Self::SMALL_INNER_CORNER_SIZE, Self::SMALL_INNER_CORNER_SIZE_PRESSED),
            (Self::MEDIUM_INNER_CORNER_SIZE, Self::MEDIUM_INNER_CORNER_SIZE_PRESSED),
            (Self::LARGE_INNER_CORNER_SIZE, Self::LARGE_INNER_CORNER_SIZE_PRESSED),
            (Self::EXTRA_LARGE_INNER_CORNER_SIZE, Self::EXTRA_LARGE_INNER_CORNER_SIZE_PRESSED),
        ][Self::nearest_size(button_height)]
    }

    /// The leading button's shapes at `button_height`, as `leadingButtonShapesFor`: round at the
    /// start, the size's inner corners at the end, rounder pressed.
    pub fn leading_button_shapes_for(button_height: Dp) -> SplitButtonShapes {
        let (inner, pressed) = Self::inner_corners(button_height);
        let outer = Self::OUTER_CORNER_SIZE;
        let shape = |inner| Shape::Rounded([outer, inner, inner, outer]);
        SplitButtonShapes {
            shape: shape(inner),
            pressed_shape: Some(shape(pressed)),
            checked_shape: None,
        }
    }

    /// The trailing button's, as `trailingButtonShapesFor`: the inner corners at the start, a pill
    /// checked.
    pub fn trailing_button_shapes_for(button_height: Dp) -> SplitButtonShapes {
        let (inner, pressed) = Self::inner_corners(button_height);
        let outer = Self::OUTER_CORNER_SIZE;
        let shape = |inner| Shape::Rounded([inner, outer, outer, inner]);
        SplitButtonShapes {
            shape: shape(inner),
            pressed_shape: Some(shape(pressed)),
            checked_shape: Some(compose_foundation::CIRCLE_SHAPE),
        }
    }

    /// As `leadingButtonContentPaddingFor`.
    pub fn leading_button_content_padding_for(button_height: Dp) -> PaddingValues {
        [
            Self::EXTRA_SMALL_LEADING_BUTTON_CONTENT_PADDING,
            Self::SMALL_LEADING_BUTTON_CONTENT_PADDING,
            Self::MEDIUM_LEADING_BUTTON_CONTENT_PADDING,
            Self::LARGE_LEADING_BUTTON_CONTENT_PADDING,
            Self::EXTRA_LARGE_LEADING_BUTTON_CONTENT_PADDING,
        ][Self::reached_size(button_height)]
    }

    /// As `trailingButtonContentPaddingFor`.
    pub fn trailing_button_content_padding_for(button_height: Dp) -> PaddingValues {
        [
            Self::EXTRA_SMALL_TRAILING_BUTTON_CONTENT_PADDING,
            Self::SMALL_TRAILING_BUTTON_CONTENT_PADDING,
            Self::MEDIUM_TRAILING_BUTTON_CONTENT_PADDING,
            Self::LARGE_TRAILING_BUTTON_CONTENT_PADDING,
            Self::EXTRA_LARGE_TRAILING_BUTTON_CONTENT_PADDING,
        ][Self::reached_size(button_height)]
    }

    /// The buttons' icon, as `leadingButtonIconSizeFor`.
    pub fn leading_button_icon_size_for(button_height: Dp) -> Dp {
        ButtonDefaults::icon_size_for(button_height)
    }

    /// As `trailingButtonIconSizeFor`.
    pub fn trailing_button_icon_size_for(button_height: Dp) -> Dp {
        [
            Self::EXTRA_SMALL_TRAILING_BUTTON_ICON_SIZE,
            Self::SMALL_TRAILING_BUTTON_ICON_SIZE,
            Self::MEDIUM_TRAILING_BUTTON_ICON_SIZE,
            Self::LARGE_TRAILING_BUTTON_ICON_SIZE,
            Self::EXTRA_LARGE_TRAILING_BUTTON_ICON_SIZE,
        ][Self::reached_size(button_height)]
    }
}

/// A leading and a trailing button side by side, `spacing` apart, as Compose's
/// `SplitButtonLayout`: the leading one takes what the trailing one leaves, both as high as the
/// taller wants (within the constraints); the pair takes the touch target, not each.
#[composable]
pub fn SplitButtonLayout(
    leading_button: impl Fn() + 'static,
    trailing_button: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::SplitButtonDefaults::SPACING)] spacing: Dp,
) {
    let policy = compose_runtime::remember_keyed(spacing, || {
        compose_ui::measure_policy(move |scope, measurables, constraints| {
            measure_split(scope, measurables, constraints, spacing)
        })
    });
    let modifier = minimum_interactive_component_size(modifier);
    compose_ui::Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            let (leading, trailing) = (leading_button.clone(), trailing_button.clone());
            CompositionLocalProvider(
                LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.provides(Dp(0.0)),
                || {
                    let (leading, trailing) = (leading.clone(), trailing.clone());
                    Box(
                        Modifier,
                        Alignment::CENTER,
                        true,
                        compose_runtime::callback(move |_| leading()),
                    );
                    Box(
                        Modifier,
                        Alignment::CENTER,
                        true,
                        compose_runtime::callback(move |_| trailing()),
                    );
                },
            );
        }),
    );
}

/// `SplitButtonLayout`'s measure: its children are the leading button's box, then the trailing
/// one's.
fn measure_split(
    scope: &mut MeasureScope<'_>,
    measurables: &[Measurable],
    constraints: Constraints,
    spacing: Dp,
) -> MeasureResult {
    let [leading, trailing] = [measurables[0], measurables[1]];
    let spacing = scope.density().round_to_px(spacing);
    let trailing_width = scope.max_intrinsic_width(trailing, constraints.max_height);
    let available = if constraints.max_width == Constraints::INFINITY {
        Constraints::INFINITY
    } else {
        (constraints.max_width - (trailing_width + spacing)).max(0)
    };
    let leading_height = scope.max_intrinsic_height(leading, available);
    let trailing_height = scope.max_intrinsic_height(trailing, trailing_width);
    let height =
        leading_height.max(trailing_height).clamp(constraints.min_height, constraints.max_height);
    let trailing = scope.measure(
        trailing,
        Constraints { min_width: 0, min_height: height, max_height: height, ..constraints },
    );
    let leading_constraints = constraints.offset(-(trailing.width + spacing), 0);
    let leading = scope.measure(
        leading,
        Constraints { min_width: 0, min_height: height, max_height: height, ..leading_constraints },
    );
    let width = (leading.width + trailing.width + spacing)
        .clamp(constraints.min_width, constraints.max_width);
    let height =
        leading.height.max(trailing.height).clamp(constraints.min_height, constraints.max_height);
    scope.place_relative(&leading, 0, (height - leading.height) / 2);
    scope.place_relative(&trailing, leading.width + spacing, (height - trailing.height) / 2);
    scope.layout(width, height)
}

/// What the buttons share.
struct SplitParts {
    modifier: Modifier,
    enabled: bool,
    shapes: SplitButtonShapes,
    colors: ButtonColors,
    elevation: Option<ButtonElevation>,
    border: Option<BorderStroke>,
    content_padding: PaddingValues,
    interaction_source: Option<MutableInteractionSource>,
    /// The trailing button's, moving its content away from its rounder end.
    trailing: bool,
}

/// A split button's button, as `LeadingButton` and the two `TrailingButton`s: a surface at least
/// 48 by 40 dp around its centered content in the label style; `checked` makes it toggleable,
/// tinted with its content color while checked.
fn split_button(
    parts: SplitParts,
    on_click: compose_runtime::Callback,
    checked: Option<bool>,
    content: compose_runtime::Callback<dyn Fn(RowScope)>,
) {
    let interaction_source = remembered_interaction_source(parts.interaction_source);
    let shape = parts.shapes.for_state(interaction_source.is_pressed(), checked.unwrap_or(false));
    let (enabled, colors) = (parts.enabled, parts.colors);
    let (container, content_color) =
        (colors.container_color(enabled), colors.content_color(enabled));
    let modifier = if checked == Some(true) {
        let shape = shape.clone();
        // As `drawOutline(…, alpha = PressedStateLayerOpacity)`.
        let tint = content_color.with_alpha(content_color.alpha() * 0.1);
        parts.modifier.draw_with_content(move |scope| {
            scope.draw_content();
            let outline =
                shape.outline_in(scope.size(), &scope.density(), scope.layout_direction());
            scope.draw_outline(outline, Paint::fill(tint));
        })
    } else {
        parts.modifier
    };
    let click = SurfaceClick {
        on_click,
        enabled,
        role: Some(Role::Button),
        interaction_source,
        selected: None,
        checked,
        elevations: parts.elevation.map(|elevation| Elevations {
            default: elevation.default_elevation,
            pressed: elevation.pressed_elevation,
            focused: elevation.focused_elevation,
            hovered: elevation.hovered_elevation,
            dragged: elevation.default_elevation,
            disabled: elevation.disabled_elevation,
        }),
    };
    let (padding, trailing) = (parts.content_padding, parts.trailing);
    let label = MaterialTheme::typography().label_large.clone();
    let row_shape = shape.clone();
    surface(
        modifier,
        shape,
        container,
        content_color,
        Dp(0.0),
        Dp(0.0),
        parts.border,
        Some(click),
        compose_runtime::callback(move || {
            let (content, label, row_shape) = (content.clone(), label.clone(), row_shape.clone());
            provide_content_color_text_style(content_color, label, || {
                let row = Modifier.default_min_size(
                    SplitButtonDefaults::BUTTON_MIN_WIDTH,
                    SplitButtonDefaults::SMALL_CONTAINER_HEIGHT,
                );
                let row = if trailing { center_optically(row, row_shape, padding) } else { row };
                Row(
                    row.padding(padding),
                    Arrangement::CENTER,
                    Alignment::CENTER_VERTICALLY,
                    content.clone(),
                );
            });
        }),
    );
}

/// Moves the content away from `shape`'s rounder end, toward its squarer one, no further than `padding` on either side, as
/// `horizontalCenterOptically`.
fn center_optically(modifier: Modifier, shape: Shape, padding: PaddingValues) -> Modifier {
    if shape.corners().is_none() && !matches!(shape, Shape::Custom(_)) {
        return modifier;
    }
    modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let density = scope.density();
        let size = Size::new(placeable.width as f32, placeable.height as f32);
        let offset = crate::animated_shape::optical_center_offset(&shape, size, &density)
            .unwrap_or(0.0)
            .clamp(-density.to_px(padding.start), density.to_px(padding.end));
        scope.place_relative(&placeable, offset.round() as i32, 0);
        scope.layout(placeable.width, placeable.height)
    })
}

/// A split button's leading button, as `SplitButtonDefaults.LeadingButton`: filled, round at the
/// start.
#[composable]
pub fn SplitButtonLeadingButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::SplitButtonDefaults::leading_button_shapes_for(
        crate::SplitButtonDefaults::SMALL_CONTAINER_HEIGHT
    ))]
    shapes: SplitButtonShapes,
    #[default(crate::ButtonDefaults::button_colors())] colors: ButtonColors,
    #[default(Some(crate::ButtonDefaults::button_elevation()))] elevation: Option<ButtonElevation>,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::SplitButtonDefaults::leading_button_content_padding_for(
        crate::SplitButtonDefaults::SMALL_CONTAINER_HEIGHT
    ))]
    content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    let parts = SplitParts {
        modifier,
        enabled,
        shapes,
        colors,
        elevation,
        border,
        content_padding,
        interaction_source,
        trailing: false,
    };
    split_button(parts, on_click, None, content);
}

/// A split button's trailing button, as `SplitButtonDefaults.TrailingButton(onClick, …)`.
#[composable]
pub fn SplitButtonTrailingButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::SplitButtonDefaults::trailing_button_shapes_for(
        crate::SplitButtonDefaults::SMALL_CONTAINER_HEIGHT
    ))]
    shapes: SplitButtonShapes,
    #[default(crate::ButtonDefaults::button_colors())] colors: ButtonColors,
    #[default(Some(crate::ButtonDefaults::button_elevation()))] elevation: Option<ButtonElevation>,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::SplitButtonDefaults::trailing_button_content_padding_for(
        crate::SplitButtonDefaults::SMALL_CONTAINER_HEIGHT
    ))]
    content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    let parts = SplitParts {
        modifier,
        enabled,
        shapes,
        colors,
        elevation,
        border,
        content_padding,
        interaction_source,
        trailing: true,
    };
    split_button(parts, on_click, None, content);
}

/// A split button's trailing button with a checked state (a menu open), as
/// `SplitButtonDefaults.TrailingButton(checked, onCheckedChange, …)`: a pill while checked.
#[composable]
pub fn SplitButtonToggleableTrailingButton(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::SplitButtonDefaults::trailing_button_shapes_for(
        crate::SplitButtonDefaults::SMALL_CONTAINER_HEIGHT
    ))]
    shapes: SplitButtonShapes,
    #[default(crate::ButtonDefaults::button_colors())] colors: ButtonColors,
    #[default(Some(crate::ButtonDefaults::button_elevation()))] elevation: Option<ButtonElevation>,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::SplitButtonDefaults::trailing_button_content_padding_for(
        crate::SplitButtonDefaults::SMALL_CONTAINER_HEIGHT
    ))]
    content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    let parts = SplitParts {
        modifier,
        enabled,
        shapes,
        colors,
        elevation,
        border,
        content_padding,
        interaction_source,
        trailing: true,
    };
    split_button(
        parts,
        compose_runtime::callback(move || on_checked_change(!checked)),
        Some(checked),
        content,
    );
}

/// The leading and trailing buttons of the other kinds: the filled ones with their kind's colors,
/// elevation and (outlined) border, as `TonalLeadingButton` and its siblings.
macro_rules! kind_buttons {
    ($(
        $(#[$leading_doc:meta])* $leading:ident,
        $(#[$trailing_doc:meta])* $trailing:ident,
        $colors:path,
        $elevation:expr,
        $border:expr;
    )*) => {$(
        $(#[$leading_doc])*
        #[composable]
        pub fn $leading(
            on_click: impl Fn() + 'static,
            #[default(crate::Modifier)] modifier: Modifier,
            #[default(true)] enabled: bool,
            #[default(crate::SplitButtonDefaults::leading_button_shapes_for(crate::SplitButtonDefaults::SMALL_CONTAINER_HEIGHT))]
            shapes: SplitButtonShapes,
            #[default($colors())] colors: ButtonColors,
            #[default($elevation)] elevation: Option<ButtonElevation>,
            #[default(None)] border: Option<BorderStroke>,
            #[default(crate::SplitButtonDefaults::leading_button_content_padding_for(crate::SplitButtonDefaults::SMALL_CONTAINER_HEIGHT))]
            content_padding: PaddingValues,
            #[default(None)] interaction_source: Option<MutableInteractionSource>,
            content: impl Fn(RowScope) + 'static,
        ) {
            let border: Option<BorderStroke> = border.or_else(|| ($border)(enabled));
            SplitButtonLeadingButton(
                on_click, modifier, enabled, shapes, colors, elevation, border, content_padding,
                interaction_source, content,
            );
        }

        $(#[$trailing_doc])*
        #[composable]
        pub fn $trailing(
            checked: bool,
            on_checked_change: impl Fn(bool) + 'static,
            #[default(crate::Modifier)] modifier: Modifier,
            #[default(true)] enabled: bool,
            #[default(crate::SplitButtonDefaults::trailing_button_shapes_for(crate::SplitButtonDefaults::SMALL_CONTAINER_HEIGHT))]
            shapes: SplitButtonShapes,
            #[default($colors())] colors: ButtonColors,
            #[default($elevation)] elevation: Option<ButtonElevation>,
            #[default(None)] border: Option<BorderStroke>,
            #[default(crate::SplitButtonDefaults::trailing_button_content_padding_for(crate::SplitButtonDefaults::SMALL_CONTAINER_HEIGHT))]
            content_padding: PaddingValues,
            #[default(None)] interaction_source: Option<MutableInteractionSource>,
            content: impl Fn(RowScope) + 'static,
        ) {
            let border: Option<BorderStroke> = border.or_else(|| ($border)(enabled));
            SplitButtonToggleableTrailingButton(
                checked, on_checked_change, modifier, enabled, shapes, colors, elevation, border,
                content_padding, interaction_source, content,
            );
        }
    )*};
}

fn no_border(_: bool) -> Option<BorderStroke> {
    None
}

fn outlined_border(enabled: bool) -> Option<BorderStroke> {
    Some(ButtonDefaults::outlined_button_border(enabled))
}

kind_buttons! {
    /// In the secondary container, as `SplitButtonDefaults.TonalLeadingButton`.
    SplitButtonTonalLeadingButton,
    /// As `SplitButtonDefaults.TonalTrailingButton`.
    SplitButtonTonalTrailingButton,
    crate::ButtonDefaults::filled_tonal_button_colors,
    Some(crate::ButtonDefaults::filled_tonal_button_elevation()),
    no_border;
    /// On a shadow, as `SplitButtonDefaults.ElevatedLeadingButton`.
    SplitButtonElevatedLeadingButton,
    /// As `SplitButtonDefaults.ElevatedTrailingButton`.
    SplitButtonElevatedTrailingButton,
    crate::ButtonDefaults::elevated_button_colors,
    Some(crate::ButtonDefaults::elevated_button_elevation()),
    no_border;
    /// Outlined, as `SplitButtonDefaults.OutlinedLeadingButton`: `border` `None` for the outlined
    /// button's (a default can't read `enabled`).
    SplitButtonOutlinedLeadingButton,
    /// As `SplitButtonDefaults.OutlinedTrailingButton`.
    SplitButtonOutlinedTrailingButton,
    crate::ButtonDefaults::outlined_button_colors,
    None,
    outlined_border;
}
