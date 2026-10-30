//! Chips and badges, as Compose's Material 3: `AssistChip`, `FilterChip`, `InputChip`,
//! `SuggestionChip` and the elevated ones, with their defaults, colors, elevations and shapes;
//! `Badge` and `BadgedBox`.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::{
    AnimationSpec, MutableTransitionState, expand_horizontally_with, fade_in_with, fade_out_with,
    shrink_horizontally_with,
};
use compose_foundation::BorderStroke;
use compose_foundation::layout::{
    Box, HorizontalArrangement, HorizontalArrangementPolicy, PaddingValues, Row, Spacer,
};
use compose_runtime::{Callback, CompositionLocalProvider, composable, key, remember};
use compose_ui::{
    Alignment, Color, Constraints, Density, Dp, IntrinsicSize, Layout, LayoutDirection, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, MutableInteractionSource, Role, Shape,
    SizeSpec, UnitExt,
};

use crate::components::{Elevations, SurfaceClick, remembered_interaction_source, surface};
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

// ---- Colors, elevations, shapes ----------------------------------------------------------------

/// A chip's colors, as Compose's `ChipColors`. Change some of the defaults with Rust's struct update
/// syntax, where Compose's `assistChipColors(containerColor = …)` names them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ChipColors {
    pub container_color: Color,
    pub label_color: Color,
    pub leading_icon_content_color: Color,
    pub trailing_icon_content_color: Color,
    pub disabled_container_color: Color,
    pub disabled_label_color: Color,
    pub disabled_leading_icon_content_color: Color,
    pub disabled_trailing_icon_content_color: Color,
}

impl ChipColors {
    /// As `containerColor(enabled)`.
    pub fn container_color(&self, enabled: bool) -> Color {
        if enabled { self.container_color } else { self.disabled_container_color }
    }

    /// As `labelColor(enabled)`.
    pub fn label_color(&self, enabled: bool) -> Color {
        if enabled { self.label_color } else { self.disabled_label_color }
    }

    /// As `leadingIconContentColor(enabled)`.
    pub fn leading_icon_content_color(&self, enabled: bool) -> Color {
        if enabled {
            self.leading_icon_content_color
        } else {
            self.disabled_leading_icon_content_color
        }
    }

    /// As `trailingIconContentColor(enabled)`.
    pub fn trailing_icon_content_color(&self, enabled: bool) -> Color {
        if enabled {
            self.trailing_icon_content_color
        } else {
            self.disabled_trailing_icon_content_color
        }
    }
}

impl compose_runtime::ComposeArg for ChipColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A selectable chip's colors, as Compose's `SelectableChipColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SelectableChipColors {
    pub container_color: Color,
    pub label_color: Color,
    pub leading_icon_color: Color,
    pub trailing_icon_color: Color,
    pub disabled_container_color: Color,
    pub disabled_label_color: Color,
    pub disabled_leading_icon_color: Color,
    pub disabled_trailing_icon_color: Color,
    pub selected_container_color: Color,
    pub disabled_selected_container_color: Color,
    pub selected_label_color: Color,
    pub selected_leading_icon_color: Color,
    pub selected_trailing_icon_color: Color,
}

impl SelectableChipColors {
    /// As `containerColor(enabled, selected)`.
    pub fn container_color(&self, enabled: bool, selected: bool) -> Color {
        match (enabled, selected) {
            (false, false) => self.disabled_container_color,
            (false, true) => self.disabled_selected_container_color,
            (true, false) => self.container_color,
            (true, true) => self.selected_container_color,
        }
    }

    /// As `labelColor(enabled, selected)`.
    pub fn label_color(&self, enabled: bool, selected: bool) -> Color {
        match (enabled, selected) {
            (false, _) => self.disabled_label_color,
            (true, false) => self.label_color,
            (true, true) => self.selected_label_color,
        }
    }

    /// As `leadingIconContentColor(enabled, selected)`.
    pub fn leading_icon_content_color(&self, enabled: bool, selected: bool) -> Color {
        match (enabled, selected) {
            (false, _) => self.disabled_leading_icon_color,
            (true, false) => self.leading_icon_color,
            (true, true) => self.selected_leading_icon_color,
        }
    }

    /// As `trailingIconContentColor(enabled, selected)`.
    pub fn trailing_icon_content_color(&self, enabled: bool, selected: bool) -> Color {
        match (enabled, selected) {
            (false, _) => self.disabled_trailing_icon_color,
            (true, false) => self.trailing_icon_color,
            (true, true) => self.selected_trailing_icon_color,
        }
    }
}

impl compose_runtime::ComposeArg for SelectableChipColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A chip's elevation in each of its states, as Compose's `ChipElevation`: its shadow, animated
/// (`surface`). The struct is the same for a selectable chip, Compose's `SelectableChipElevation`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ChipElevation {
    pub elevation: Dp,
    pub pressed_elevation: Dp,
    pub focused_elevation: Dp,
    pub hovered_elevation: Dp,
    pub dragged_elevation: Dp,
    pub disabled_elevation: Dp,
}

/// As Compose's `SelectableChipElevation`.
pub type SelectableChipElevation = ChipElevation;

impl ChipElevation {
    /// All at `elevation`, dragged at the 8 dp of the tokens: the flat chips', as
    /// `assistChipElevation(elevation)`, `suggestionChipElevation(elevation)` and
    /// `inputChipElevation(elevation)`, whose other parameters default to it. Change others with
    /// struct update.
    pub const fn flat(elevation: Dp) -> ChipElevation {
        ChipElevation {
            elevation,
            pressed_elevation: elevation,
            focused_elevation: elevation,
            hovered_elevation: elevation,
            dragged_elevation: Dp(8.0),
            disabled_elevation: elevation,
        }
    }

    /// The elevated chips' tokens.
    const ELEVATED: ChipElevation = ChipElevation {
        elevation: Dp(1.0),
        pressed_elevation: Dp(1.0),
        focused_elevation: Dp(1.0),
        hovered_elevation: Dp(3.0),
        dragged_elevation: Dp(8.0),
        disabled_elevation: Dp(0.0),
    };

    fn elevations(&self) -> Elevations {
        Elevations {
            default: self.elevation,
            pressed: self.pressed_elevation,
            focused: self.focused_elevation,
            hovered: self.hovered_elevation,
            dragged: self.dragged_elevation,
            disabled: self.disabled_elevation,
        }
    }
}

impl compose_runtime::ComposeArg for ChipElevation {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A selectable chip's shape in each state, as Compose's `ChipShapes`: its corners move between
/// them, if they all have corners.
#[derive(Clone, PartialEq, Debug)]
pub struct ChipShapes {
    pub shape: Shape,
    pub selected_shape: Shape,
    pub pressed_shape: Shape,
}

impl compose_runtime::ComposeArg for ChipShapes {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl std::hash::Hash for ChipShapes {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        crate::animated_shape::hash_shapes(
            [&self.shape, &self.selected_shape, &self.pressed_shape],
            state,
        );
    }
}

impl ChipShapes {
    /// As `hasCornerBasedShapes`.
    fn corner_based(&self) -> bool {
        [&self.shape, &self.selected_shape, &self.pressed_shape]
            .into_iter()
            .all(|shape| shape.corners().is_some())
    }

    /// As `shapeByInteraction`.
    fn for_state(&self, selected: bool, pressed: bool, spec: AnimationSpec) -> Shape {
        let target = if pressed {
            self.pressed_shape.clone()
        } else if selected {
            self.selected_shape.clone()
        } else {
            self.shape.clone()
        };
        key(self.clone(), || {
            if self.corner_based() {
                crate::animated_shape::remember_animated_shape(target, spec)
            } else {
                target
            }
        })
    }
}

/// What a flat chip's `border` takes: its default for its state (a default can't read `enabled`),
/// none, or one's own. A `BorderStroke` or an `Option` of one converts, `None` being no border as
/// Compose's. Not Compose's deprecated `ChipBorder`, whose place it doesn't take (D1039).
#[derive(Clone, PartialEq, Debug)]
pub enum ChipBorderChoice {
    Default,
    None,
    Stroke(BorderStroke),
}

impl From<BorderStroke> for ChipBorderChoice {
    fn from(border: BorderStroke) -> ChipBorderChoice {
        ChipBorderChoice::Stroke(border)
    }
}

impl From<Option<BorderStroke>> for ChipBorderChoice {
    fn from(border: Option<BorderStroke>) -> ChipBorderChoice {
        border.map_or(ChipBorderChoice::None, ChipBorderChoice::Stroke)
    }
}

impl compose_runtime::ComposeArg for ChipBorderChoice {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ChipBorderChoice {
    fn or_else(self, default: impl FnOnce() -> BorderStroke) -> Option<BorderStroke> {
        match self {
            ChipBorderChoice::Default => Some(default()),
            ChipBorderChoice::None => None,
            ChipBorderChoice::Stroke(border) => Some(border),
        }
    }
}

// ---- The arrangement ---------------------------------------------------------------------------

/// The chips' three children: `leading`, the space after a leading child (or `trailing` with none),
/// the label, and the last child at the end, as `ChipArrangement`.
#[derive(PartialEq)]
struct ChipArrangement {
    leading: Dp,
    trailing: Dp,
}

impl HorizontalArrangementPolicy for ChipArrangement {
    fn spacing(&self) -> Dp {
        Dp((self.leading.0 + self.trailing.0) / 2.0)
    }

    fn arrange(
        &self,
        density: &Density,
        total_size: i32,
        sizes: &[i32],
        layout_direction: LayoutDirection,
        out_positions: &mut [i32],
    ) {
        let (leading, trailing) =
            (density.round_to_px(self.leading), density.round_to_px(self.trailing));
        for (index, size) in sizes.iter().enumerate() {
            let at = match index {
                1 => sizes[0] + if sizes[0] > 0 { leading } else { trailing },
                2 => total_size - size,
                _ => 0,
            };
            out_positions[index] =
                if layout_direction == LayoutDirection::Ltr { at } else { total_size - at - size };
        }
    }

    fn same(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>() == Some(self)
    }
}

fn chip_arrangement(leading: Dp, trailing: Dp) -> HorizontalArrangement {
    thread_local! {
        /// `DefaultHorizontalArrangement`: each chip's, so one.
        static DEFAULT: HorizontalArrangement = HorizontalArrangement::custom(ChipArrangement {
            leading: HORIZONTAL_SPACING,
            trailing: HORIZONTAL_SPACING,
        });
    }
    if leading == HORIZONTAL_SPACING && trailing == HORIZONTAL_SPACING {
        DEFAULT.with(HorizontalArrangement::clone)
    } else {
        HorizontalArrangement::custom(ChipArrangement { leading, trailing })
    }
}

const HORIZONTAL_SPACING: Dp = Dp(8.0);
const COMPACT_HORIZONTAL_SPACING: Dp = Dp(4.0);
/// `Max width for a chip`, as `maxChipWidth`: so a selectable chip's animations have a width to work
/// with in a scrolling container.
const MAX_CHIP_WIDTH: Dp = Dp(1000.0);

// ---- The defaults ------------------------------------------------------------------------------

/// As Compose's `AssistChipDefaults`.
pub struct AssistChipDefaults {}

impl AssistChipDefaults {
    pub const HEIGHT: Dp = Dp(32.0);
    pub const ICON_SIZE: Dp = Dp(18.0);
    pub const HORIZONTAL_SPACING: Dp = HORIZONTAL_SPACING;
    pub const CONTENT_PADDING: PaddingValues = PaddingValues::symmetric(Dp(8.0), Dp(0.0));

    /// As `horizontalArrangement()`.
    pub fn horizontal_arrangement() -> HorizontalArrangement {
        chip_arrangement(HORIZONTAL_SPACING, HORIZONTAL_SPACING)
    }

    /// As `horizontalArrangement(spacing)`.
    pub fn horizontal_arrangement_with(spacing: Dp) -> HorizontalArrangement {
        chip_arrangement(spacing, spacing)
    }

    /// As `assistChipColors()`.
    pub fn assist_chip_colors() -> ChipColors {
        let scheme = MaterialTheme::color_scheme();
        let dim = scheme.on_surface.with_alpha(0.38);
        ChipColors {
            container_color: Color::TRANSPARENT,
            label_color: scheme.on_surface,
            leading_icon_content_color: scheme.primary,
            trailing_icon_content_color: scheme.primary,
            disabled_container_color: Color::TRANSPARENT,
            disabled_label_color: dim,
            disabled_leading_icon_content_color: dim,
            disabled_trailing_icon_content_color: dim,
        }
    }

    /// As `assistChipElevation()`.
    pub fn assist_chip_elevation() -> ChipElevation {
        ChipElevation::flat(Dp(0.0))
    }

    /// As `assistChipBorderChoice(enabled)`.
    pub fn assist_chip_border(enabled: bool) -> BorderStroke {
        let scheme = MaterialTheme::color_scheme();
        BorderStroke::new(
            Dp(1.0),
            if enabled { scheme.outline_variant } else { scheme.on_surface.with_alpha(0.12) },
        )
    }

    /// As `assistChipBorder(enabled, borderColor, disabledBorderColor, borderWidth)`.
    pub fn assist_chip_border_with(
        enabled: bool,
        border_color: Color,
        disabled_border_color: Color,
        border_width: Dp,
    ) -> BorderStroke {
        BorderStroke::new(border_width, if enabled { border_color } else { disabled_border_color })
    }

    /// As `elevatedAssistChipColors()`.
    pub fn elevated_assist_chip_colors() -> ChipColors {
        let scheme = MaterialTheme::color_scheme();
        ChipColors {
            container_color: scheme.surface_container_low,
            disabled_container_color: scheme.on_surface.with_alpha(0.12),
            ..AssistChipDefaults::assist_chip_colors()
        }
    }

    /// As `elevatedAssistChipElevation()`.
    pub fn elevated_assist_chip_elevation() -> ChipElevation {
        ChipElevation::ELEVATED
    }

    /// As `shape`.
    pub fn shape() -> Shape {
        MaterialTheme::shapes().small.clone()
    }
}

/// As Compose's `FilterChipDefaults`.
pub struct FilterChipDefaults {}

impl FilterChipDefaults {
    pub const HEIGHT: Dp = Dp(32.0);
    pub const ICON_SIZE: Dp = Dp(18.0);
    pub const HORIZONTAL_SPACING: Dp = HORIZONTAL_SPACING;
    pub const COMPACT_HORIZONTAL_SPACING: Dp = COMPACT_HORIZONTAL_SPACING;
    pub const CONTENT_PADDING: PaddingValues = PaddingValues::symmetric(Dp(8.0), Dp(0.0));

    /// As `horizontalArrangement()`.
    pub fn horizontal_arrangement() -> HorizontalArrangement {
        chip_arrangement(HORIZONTAL_SPACING, HORIZONTAL_SPACING)
    }

    /// As `horizontalArrangement(hasLeadingIcon, hasTrailingIcon)`: compact beside an icon.
    pub fn horizontal_arrangement_for(
        has_leading_icon: bool,
        has_trailing_icon: bool,
    ) -> HorizontalArrangement {
        let (compact, regular) = (COMPACT_HORIZONTAL_SPACING, HORIZONTAL_SPACING);
        chip_arrangement(
            if has_leading_icon { compact } else { regular },
            if has_trailing_icon { compact } else { regular },
        )
    }

    /// As `horizontalArrangement(spacing)`.
    pub fn horizontal_arrangement_with(spacing: Dp) -> HorizontalArrangement {
        chip_arrangement(spacing, spacing)
    }

    /// As `filterChipColors()`.
    pub fn filter_chip_colors() -> SelectableChipColors {
        let scheme = MaterialTheme::color_scheme();
        let dim = scheme.on_surface.with_alpha(0.38);
        SelectableChipColors {
            container_color: Color::TRANSPARENT,
            label_color: scheme.on_surface_variant,
            leading_icon_color: scheme.primary,
            trailing_icon_color: scheme.on_surface_variant,
            disabled_container_color: Color::TRANSPARENT,
            disabled_label_color: dim,
            disabled_leading_icon_color: dim,
            disabled_trailing_icon_color: dim,
            selected_container_color: scheme.secondary_container,
            disabled_selected_container_color: scheme.on_surface.with_alpha(0.12),
            selected_label_color: scheme.on_secondary_container,
            selected_leading_icon_color: scheme.on_secondary_container,
            selected_trailing_icon_color: scheme.on_secondary_container,
        }
    }

    /// As `tonalFilterChipColors()`.
    pub fn tonal_filter_chip_colors() -> SelectableChipColors {
        SelectableChipColors {
            leading_icon_color: MaterialTheme::color_scheme().on_surface_variant,
            ..FilterChipDefaults::filter_chip_colors()
        }
    }

    /// As `filterChipElevation()`: a 1 dp shadow on a hover.
    pub fn filter_chip_elevation() -> SelectableChipElevation {
        ChipElevation { hovered_elevation: Dp(1.0), ..ChipElevation::flat(Dp(0.0)) }
    }

    /// As `filterChipBorderChoice(enabled, selected)`: selected, a hairline nobody sees.
    pub fn filter_chip_border(enabled: bool, selected: bool) -> BorderStroke {
        selectable_border(enabled, selected)
    }

    /// As `filterChipBorder(enabled, selected, borderColor, selectedBorderColor, disabledBorderColor,
    /// disabledSelectedBorderColor, borderWidth, selectedBorderWidth)`.
    #[allow(clippy::too_many_arguments)]
    pub fn filter_chip_border_with(
        enabled: bool,
        selected: bool,
        border_color: Color,
        selected_border_color: Color,
        disabled_border_color: Color,
        disabled_selected_border_color: Color,
        border_width: Dp,
        selected_border_width: Dp,
    ) -> BorderStroke {
        let color = match (enabled, selected) {
            (true, false) => border_color,
            (true, true) => selected_border_color,
            (false, false) => disabled_border_color,
            (false, true) => disabled_selected_border_color,
        };
        BorderStroke::new(if selected { selected_border_width } else { border_width }, color)
    }

    /// As `elevatedFilterChipColors()`.
    pub fn elevated_filter_chip_colors() -> SelectableChipColors {
        let scheme = MaterialTheme::color_scheme();
        let faint = scheme.on_surface.with_alpha(0.12);
        SelectableChipColors {
            container_color: scheme.surface_container_low,
            disabled_container_color: faint,
            selected_container_color: scheme.secondary_container,
            disabled_selected_container_color: faint,
            ..FilterChipDefaults::filter_chip_colors()
        }
    }

    /// As `tonalElevatedFilterChipColors()`.
    pub fn tonal_elevated_filter_chip_colors() -> SelectableChipColors {
        SelectableChipColors {
            leading_icon_color: MaterialTheme::color_scheme().on_surface_variant,
            ..FilterChipDefaults::elevated_filter_chip_colors()
        }
    }

    /// As `elevatedFilterChipElevation()`.
    pub fn elevated_filter_chip_elevation() -> SelectableChipElevation {
        ChipElevation::ELEVATED
    }

    /// As `shape`.
    pub fn shape() -> Shape {
        MaterialTheme::shapes().small.clone()
    }

    /// As `shapes()`.
    pub fn shapes() -> ChipShapes {
        default_chip_shapes()
    }
}

fn default_chip_shapes() -> ChipShapes {
    let shapes = MaterialTheme::shapes();
    ChipShapes {
        shape: shapes.medium.clone(),
        selected_shape: compose_foundation::CIRCLE_SHAPE,
        pressed_shape: shapes.small.clone(),
    }
}

/// `filterChipBorderChoice` and `inputChipBorderChoice`, the same tokens: 1 dp unselected, a transparent
/// hairline selected.
fn selectable_border(enabled: bool, selected: bool) -> BorderStroke {
    let scheme = MaterialTheme::color_scheme();
    if selected {
        BorderStroke::new(Dp(0.0), Color::TRANSPARENT)
    } else {
        BorderStroke::new(
            Dp(1.0),
            if enabled { scheme.outline_variant } else { scheme.on_surface.with_alpha(0.12) },
        )
    }
}

/// As Compose's `InputChipDefaults`.
pub struct InputChipDefaults {}

impl InputChipDefaults {
    pub const HEIGHT: Dp = Dp(32.0);
    pub const ICON_SIZE: Dp = Dp(18.0);
    pub const AVATAR_SIZE: Dp = Dp(24.0);
    pub const HORIZONTAL_SPACING: Dp = HORIZONTAL_SPACING;
    pub const COMPACT_HORIZONTAL_SPACING: Dp = COMPACT_HORIZONTAL_SPACING;

    /// As `horizontalArrangement()`.
    pub fn horizontal_arrangement() -> HorizontalArrangement {
        chip_arrangement(HORIZONTAL_SPACING, HORIZONTAL_SPACING)
    }

    /// As `horizontalArrangement(hasAvatar, hasLeadingIcon, hasTrailingIcon)`.
    pub fn horizontal_arrangement_for(
        has_avatar: bool,
        has_leading_icon: bool,
        has_trailing_icon: bool,
    ) -> HorizontalArrangement {
        FilterChipDefaults::horizontal_arrangement_for(
            has_avatar || has_leading_icon,
            has_trailing_icon,
        )
    }

    /// As `horizontalArrangement(spacing)`.
    pub fn horizontal_arrangement_with(spacing: Dp) -> HorizontalArrangement {
        chip_arrangement(spacing, spacing)
    }

    /// As `contentPadding(hasAvatar, hasLeadingIcon, hasTrailingIcon)`.
    pub fn content_padding(
        has_avatar: bool,
        has_leading_icon: bool,
        has_trailing_icon: bool,
    ) -> PaddingValues {
        PaddingValues::new(
            if has_avatar || !has_leading_icon { Dp(4.0) } else { Dp(8.0) },
            Dp(0.0),
            if has_trailing_icon { Dp(8.0) } else { Dp(4.0) },
            Dp(0.0),
        )
    }

    /// As `inputChipColors()`.
    pub fn input_chip_colors() -> SelectableChipColors {
        let scheme = MaterialTheme::color_scheme();
        SelectableChipColors {
            leading_icon_color: scheme.on_surface_variant,
            selected_leading_icon_color: scheme.primary,
            ..FilterChipDefaults::filter_chip_colors()
        }
    }

    /// As `tonalInputChipColors()`.
    pub fn tonal_input_chip_colors() -> SelectableChipColors {
        InputChipDefaults::input_chip_colors()
    }

    /// As `inputChipElevation()`.
    pub fn input_chip_elevation() -> SelectableChipElevation {
        ChipElevation::flat(Dp(0.0))
    }

    /// As `inputChipBorderChoice(enabled, selected)`.
    pub fn input_chip_border(enabled: bool, selected: bool) -> BorderStroke {
        selectable_border(enabled, selected)
    }

    /// As `inputChipBorder(enabled, selected, borderColor, selectedBorderColor, disabledBorderColor,
    /// disabledSelectedBorderColor, borderWidth, selectedBorderWidth)`.
    #[allow(clippy::too_many_arguments)]
    pub fn input_chip_border_with(
        enabled: bool,
        selected: bool,
        border_color: Color,
        selected_border_color: Color,
        disabled_border_color: Color,
        disabled_selected_border_color: Color,
        border_width: Dp,
        selected_border_width: Dp,
    ) -> BorderStroke {
        FilterChipDefaults::filter_chip_border_with(
            enabled,
            selected,
            border_color,
            selected_border_color,
            disabled_border_color,
            disabled_selected_border_color,
            border_width,
            selected_border_width,
        )
    }

    /// As `shape`.
    pub fn shape() -> Shape {
        MaterialTheme::shapes().small.clone()
    }

    /// As `shapes()`.
    pub fn shapes() -> ChipShapes {
        default_chip_shapes()
    }
}

/// As Compose's `SuggestionChipDefaults`.
pub struct SuggestionChipDefaults {}

impl SuggestionChipDefaults {
    pub const HEIGHT: Dp = Dp(32.0);
    pub const ICON_SIZE: Dp = Dp(18.0);
    pub const HORIZONTAL_SPACING: Dp = HORIZONTAL_SPACING;
    pub const CONTENT_PADDING: PaddingValues = PaddingValues::symmetric(Dp(8.0), Dp(0.0));

    /// As `horizontalArrangement()`.
    pub fn horizontal_arrangement() -> HorizontalArrangement {
        chip_arrangement(HORIZONTAL_SPACING, HORIZONTAL_SPACING)
    }

    /// As `horizontalArrangement(spacing)`.
    pub fn horizontal_arrangement_with(spacing: Dp) -> HorizontalArrangement {
        chip_arrangement(spacing, spacing)
    }

    /// As `suggestionChipColors()`.
    pub fn suggestion_chip_colors() -> ChipColors {
        let scheme = MaterialTheme::color_scheme();
        let dim = scheme.on_surface.with_alpha(0.38);
        ChipColors {
            container_color: Color::TRANSPARENT,
            label_color: scheme.on_surface_variant,
            leading_icon_content_color: scheme.primary,
            trailing_icon_content_color: Color::UNSPECIFIED,
            disabled_container_color: Color::TRANSPARENT,
            disabled_label_color: dim,
            disabled_leading_icon_content_color: dim,
            disabled_trailing_icon_content_color: Color::UNSPECIFIED,
        }
    }

    /// As `suggestionChipElevation()`.
    pub fn suggestion_chip_elevation() -> ChipElevation {
        ChipElevation::flat(Dp(0.0))
    }

    /// As `suggestionChipBorderChoice(enabled)`.
    pub fn suggestion_chip_border(enabled: bool) -> BorderStroke {
        AssistChipDefaults::assist_chip_border(enabled)
    }

    /// As `suggestionChipBorder(enabled, borderColor, disabledBorderColor, borderWidth)`.
    pub fn suggestion_chip_border_with(
        enabled: bool,
        border_color: Color,
        disabled_border_color: Color,
        border_width: Dp,
    ) -> BorderStroke {
        AssistChipDefaults::assist_chip_border_with(
            enabled,
            border_color,
            disabled_border_color,
            border_width,
        )
    }

    /// As `elevatedSuggestionChipColors()`.
    pub fn elevated_suggestion_chip_colors() -> ChipColors {
        let scheme = MaterialTheme::color_scheme();
        ChipColors {
            container_color: scheme.surface_container_low,
            disabled_container_color: scheme.on_surface.with_alpha(0.12),
            ..SuggestionChipDefaults::suggestion_chip_colors()
        }
    }

    /// As `elevatedSuggestionChipElevation()`.
    pub fn elevated_suggestion_chip_elevation() -> ChipElevation {
        ChipElevation::ELEVATED
    }

    /// As `shape`.
    pub fn shape() -> Shape {
        MaterialTheme::shapes().small.clone()
    }
}

// ---- The bodies --------------------------------------------------------------------------------

/// What every chip's body takes.
struct ChipParts {
    modifier: Modifier,
    enabled: bool,
    on_click: Callback,
    label: Callback,
    label_color: Color,
    leading: Option<Callback>,
    /// A selectable chip's, which takes the place of `leading`.
    avatar: Option<Callback>,
    trailing: Option<Callback>,
    leading_color: Color,
    trailing_color: Color,
    shape: Shape,
    container: Color,
    elevation: Option<ChipElevation>,
    border: Option<BorderStroke>,
    arrangement: HorizontalArrangement,
    padding: PaddingValues,
    interaction_source: MutableInteractionSource,
}

/// The surface a chip's content is on, as `Chip` and `SelectableChip` share.
fn chip_surface(parts: ChipParts, role: Role, selected: Option<bool>, content: Callback) {
    surface(
        parts.modifier,
        parts.shape,
        parts.container,
        Color::UNSPECIFIED,
        Dp(0.0),
        Dp(0.0),
        parts.border,
        Some(SurfaceClick {
            on_click: parts.on_click,
            enabled: parts.enabled,
            role: Some(role),
            interaction_source: parts.interaction_source,
            selected,
            checked: None,
            elevations: parts.elevation.map(|elevation| elevation.elevations()),
        }),
        content,
    );
}

/// `content` in `color`.
fn in_color(content: Callback, color: Color) -> Callback {
    compose_runtime::callback(move || {
        CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), &*content);
    })
}

/// A chip that isn't selectable, as `Chip`, with its `ChipContent`: an intrinsic width.
fn chip(parts: ChipParts) {
    let label = parts.label.clone();
    let label_color = parts.label_color;
    let leading = parts.leading.clone().map(|it| in_color(it, parts.leading_color));
    let trailing = parts.trailing.clone().map(|it| in_color(it, parts.trailing_color));
    let (arrangement, padding) = (parts.arrangement.clone(), parts.padding);
    let content = compose_runtime::callback(move || {
        let (label, leading, trailing, arrangement) =
            (label.clone(), leading.clone(), trailing.clone(), arrangement.clone());
        provide_label_style(label_color, move || {
            let (label, leading, trailing) = (label.clone(), leading.clone(), trailing.clone());
            Row(
                Modifier
                    .width(IntrinsicSize::Max)
                    .default_min_size(Dp::UNSPECIFIED, AssistChipDefaults::HEIGHT)
                    .padding(padding),
                arrangement.clone(),
                Alignment::CENTER_VERTICALLY,
                compose_runtime::callback(move |scope| {
                    icon_box(leading.clone());
                    let label = label.clone();
                    Row(
                        Modifier.weight(scope, 1.0),
                        compose_foundation::layout::Arrangement::START,
                        Alignment::CENTER_VERTICALLY,
                        compose_runtime::callback(move |_| label()),
                    );
                    icon_box(trailing.clone());
                }),
            );
        });
    });
    chip_surface(parts, Role::Button, None, content);
}

/// The text style of a chip's label, and its color.
fn provide_label_style(color: Color, content: impl Fn() + 'static) {
    let style = MaterialTheme::typography().label_large.clone();
    CompositionLocalProvider(
        (LOCAL_CONTENT_COLOR.provides(color), LOCAL_TEXT_STYLE.provides(style.clone())),
        &content,
    );
}

/// An icon centered in a box, or a spacer of no width, as `ChipContent`'s ends.
fn icon_box(icon: Option<Callback>) {
    match icon {
        Some(icon) => {
            Box(Modifier, Alignment::CENTER, false, compose_runtime::callback(move |_| icon()))
        }
        None => Spacer(Modifier.width(Dp(0.0))),
    }
}

/// The animations of a selectable chip's ends.
#[derive(Clone)]
struct IconSpecs {
    fade_in: AnimationSpec,
    fade_out: AnimationSpec,
    expand: AnimationSpec,
    shrink: AnimationSpec,
}

/// A chip that's selected or not, as `SelectableChip`, with its `AnimatingChipContent`: its icons
/// coming and going as they change from none to some.
fn selectable_chip(parts: ChipParts, selected: bool, specs: IconSpecs) {
    let label = parts.label.clone();
    let label_color = parts.label_color;
    // An avatar takes precedence, and has its own colors.
    let leading = match parts.avatar.clone() {
        Some(avatar) => Some(avatar),
        None => parts.leading.clone().map(|it| in_color(it, parts.leading_color)),
    };
    let trailing = parts.trailing.clone().map(|it| in_color(it, parts.trailing_color));
    let (arrangement, padding) = (parts.arrangement.clone(), parts.padding);
    let content = compose_runtime::callback(move || {
        let (label, leading, trailing, arrangement, specs) =
            (label.clone(), leading.clone(), trailing.clone(), arrangement.clone(), specs.clone());
        provide_label_style(label_color, move || {
            let (label, leading, trailing, specs) =
                (label.clone(), leading.clone(), trailing.clone(), specs.clone());
            Row(
                Modifier
                    .size_in(SizeSpec::bounds(
                        Dp::UNSPECIFIED,
                        Dp::UNSPECIFIED,
                        MAX_CHIP_WIDTH,
                        Dp::UNSPECIFIED,
                    ))
                    .default_min_size(Dp::UNSPECIFIED, FilterChipDefaults::HEIGHT)
                    .padding(padding),
                arrangement.clone(),
                Alignment::CENTER_VERTICALLY,
                compose_runtime::callback(move |scope| {
                    animating_icon(leading.clone(), Alignment::START, specs.clone());
                    let label = label.clone();
                    Row(
                        Modifier.weight_with(scope, 1.0, false),
                        compose_foundation::layout::Arrangement::START,
                        Alignment::CENTER_VERTICALLY,
                        compose_runtime::callback(move |_| label()),
                    );
                    animating_icon(trailing.clone(), Alignment::END, specs.clone());
                }),
            );
        });
    });
    chip_surface(parts, Role::Checkbox, Some(selected), content);
}

/// A selectable chip's end: as Compose's `Box` around an `AnimatedVisibility` on whether there's an
/// icon, keeping the last one to leave with (`rememberRetainedState`). All of that's made with the
/// first icon: until then, the spacer of no width it would hold, a chip in a long list mostly having
/// none (D1039).
fn animating_icon(
    icon: Option<Callback>,
    towards: compose_ui::HorizontalAlignment,
    specs: IconSpecs,
) {
    let visible = icon.is_some();
    let held = remember(|| {
        Rc::new(IconEnd { first: visible, ever: Cell::new(visible), last: RefCell::new(None) })
    });
    held.ever.set(held.ever.get() || visible);
    if let Some(icon) = &icon {
        *held.last.borrow_mut() = Some(icon.clone());
    }
    let ever = held.ever.get();
    key(ever, || {
        if !ever {
            Spacer(Modifier.width(Dp(0.0)));
            return;
        }
        let state = remember(|| MutableTransitionState::new(held.first));
        state.set_target_state(visible);
        Box(
            Modifier,
            Alignment::TOP_START,
            false,
            compose_runtime::callback(move |_| {
                let held = held.clone();
                let enter = expand_horizontally_with(specs.expand.clone(), towards, true, |_| 0)
                    + fade_in_with(specs.fade_in.clone(), 0.0);
                let exit = shrink_horizontally_with(specs.shrink.clone(), towards, true, |_| 0)
                    + fade_out_with(specs.fade_out.clone(), 0.0);
                compose_animation::AnimatedVisibility(
                    state.clone(),
                    Modifier,
                    Some(enter),
                    Some(exit),
                    "AnimatedVisibility",
                    compose_runtime::callback(move |_| {
                        let shown = held.last.borrow().clone();
                        Box(
                            Modifier,
                            Alignment::CENTER,
                            false,
                            compose_runtime::callback(move |_| {
                                if let Some(shown) = &shown {
                                    shown();
                                }
                            }),
                        );
                    }),
                );
                if !visible {
                    Spacer(Modifier.width(Dp(0.0)));
                }
            }),
        );
    });
}

/// What an end of a selectable chip keeps.
struct IconEnd {
    /// Whether it had an icon when it was first composed: whether that one animates in.
    first: bool,
    ever: Cell<bool>,
    last: RefCell<Option<Callback>>,
}

/// The specs of `SelectableChip` with a `shape`, and with `shapes`.
fn icon_specs(shaped: bool) -> IconSpecs {
    let motion = MaterialTheme::motion_scheme();
    if shaped {
        IconSpecs {
            fade_in: motion.default_effects_spec(),
            fade_out: motion.default_effects_spec(),
            expand: motion.fast_spatial_spec(),
            shrink: motion.fast_spatial_spec(),
        }
    } else {
        IconSpecs {
            fade_in: motion.slow_effects_spec(),
            fade_out: motion.fast_effects_spec(),
            expand: motion.fast_spatial_spec(),
            shrink: motion.default_effects_spec(),
        }
    }
}

// ---- The chips ---------------------------------------------------------------------------------

/// A chip suggesting an action, as Compose's `AssistChip`, flat. An outline, its label in the
/// label-large style 16 dp in, 8 dp beside an icon, 32 dp high in a 48 dp target.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn AssistChip(
    on_click: impl Fn() + 'static,
    label: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(crate::AssistChipDefaults::shape())] shape: Shape,
    #[default(crate::AssistChipDefaults::assist_chip_colors())] colors: ChipColors,
    #[default(Some(crate::AssistChipDefaults::assist_chip_elevation()))] elevation: Option<
        ChipElevation,
    >,
    #[default(crate::ChipBorderChoice::Default)] border: impl Into<ChipBorderChoice>,
    #[default(crate::AssistChipDefaults::horizontal_arrangement())] horizontal_arrangement: impl Into<
        HorizontalArrangement,
    >,
    #[default(crate::AssistChipDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    let border: ChipBorderChoice = border;
    let border = border.or_else(|| AssistChipDefaults::assist_chip_border(enabled));
    plain_chip(
        on_click,
        label,
        modifier,
        enabled,
        leading_icon,
        trailing_icon,
        shape,
        colors,
        elevation,
        border,
        horizontal_arrangement,
        content_padding,
        interaction_source,
    );
}

/// A chip suggesting an action, as Compose's `ElevatedAssistChip`: on a container, with a shadow.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn ElevatedAssistChip(
    on_click: impl Fn() + 'static,
    label: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(crate::AssistChipDefaults::shape())] shape: Shape,
    #[default(crate::AssistChipDefaults::elevated_assist_chip_colors())] colors: ChipColors,
    #[default(Some(crate::AssistChipDefaults::elevated_assist_chip_elevation()))] elevation: Option<
        ChipElevation,
    >,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::AssistChipDefaults::horizontal_arrangement())] horizontal_arrangement: impl Into<
        HorizontalArrangement,
    >,
    #[default(crate::AssistChipDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    plain_chip(
        on_click,
        label,
        modifier,
        enabled,
        leading_icon,
        trailing_icon,
        shape,
        colors,
        elevation,
        border,
        horizontal_arrangement,
        content_padding,
        interaction_source,
    );
}

/// A chip offering a suggestion, as Compose's `SuggestionChip`, flat.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn SuggestionChip(
    on_click: impl Fn() + 'static,
    label: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    #[default(crate::SuggestionChipDefaults::shape())] shape: Shape,
    #[default(crate::SuggestionChipDefaults::suggestion_chip_colors())] colors: ChipColors,
    #[default(Some(crate::SuggestionChipDefaults::suggestion_chip_elevation()))] elevation: Option<
        ChipElevation,
    >,
    #[default(crate::ChipBorderChoice::Default)] border: impl Into<ChipBorderChoice>,
    #[default(crate::SuggestionChipDefaults::horizontal_arrangement())]
    horizontal_arrangement: impl Into<HorizontalArrangement>,
    #[default(crate::SuggestionChipDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    let border: ChipBorderChoice = border;
    let border = border.or_else(|| SuggestionChipDefaults::suggestion_chip_border(enabled));
    plain_chip(
        on_click,
        label,
        modifier,
        enabled,
        icon,
        None,
        shape,
        colors,
        elevation,
        border,
        horizontal_arrangement,
        content_padding,
        interaction_source,
    );
}

/// A chip offering a suggestion, as Compose's `ElevatedSuggestionChip`.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn ElevatedSuggestionChip(
    on_click: impl Fn() + 'static,
    label: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    #[default(crate::SuggestionChipDefaults::shape())] shape: Shape,
    #[default(crate::SuggestionChipDefaults::elevated_suggestion_chip_colors())] colors: ChipColors,
    #[default(Some(crate::SuggestionChipDefaults::elevated_suggestion_chip_elevation()))]
    elevation: Option<ChipElevation>,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::SuggestionChipDefaults::horizontal_arrangement())]
    horizontal_arrangement: impl Into<HorizontalArrangement>,
    #[default(crate::SuggestionChipDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    plain_chip(
        on_click,
        label,
        modifier,
        enabled,
        icon,
        None,
        shape,
        colors,
        elevation,
        border,
        horizontal_arrangement,
        content_padding,
        interaction_source,
    );
}

#[allow(clippy::too_many_arguments)]
fn plain_chip(
    on_click: Callback,
    label: Callback,
    modifier: Modifier,
    enabled: bool,
    leading: Option<Callback>,
    trailing: Option<Callback>,
    shape: Shape,
    colors: ChipColors,
    elevation: Option<ChipElevation>,
    border: Option<BorderStroke>,
    arrangement: HorizontalArrangement,
    padding: PaddingValues,
    interaction_source: Option<MutableInteractionSource>,
) {
    chip(ChipParts {
        modifier,
        enabled,
        on_click,
        label,
        label_color: colors.label_color(enabled),
        leading,
        avatar: None,
        trailing,
        leading_color: colors.leading_icon_content_color(enabled),
        trailing_color: colors.trailing_icon_content_color(enabled),
        shape,
        container: colors.container_color(enabled),
        elevation,
        border,
        arrangement,
        padding,
        interaction_source: remembered_interaction_source(interaction_source),
    });
}

/// A chip that's on or off, as Compose's `FilterChip`, flat: selected, in the secondary container
/// without an outline. It shows the icons it's given only (Compose's samples lead a selected chip
/// with `icons::filled::check()`), which come and go on the motion scheme. With `shapes`, as
/// Compose's overload taking them, its corners follow its state, `colors` are by default the tonal
/// ones (`tonal_filter_chip_colors`) and its spacing is compact beside an icon.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn FilterChip(
    selected: bool,
    on_click: impl Fn() + 'static,
    label: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(crate::FilterChipDefaults::shape())] shape: Shape,
    #[default(None)] shapes: impl Into<Option<ChipShapes>>,
    #[default(None)] colors: impl Into<Option<SelectableChipColors>>,
    #[default(Some(crate::FilterChipDefaults::filter_chip_elevation()))] elevation: Option<
        SelectableChipElevation,
    >,
    #[default(crate::ChipBorderChoice::Default)] border: impl Into<ChipBorderChoice>,
    #[default(None)] horizontal_arrangement: impl Into<Option<HorizontalArrangement>>,
    #[default(crate::FilterChipDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    let shapes: Option<ChipShapes> = shapes;
    let colors: Option<SelectableChipColors> = colors;
    let colors = colors.unwrap_or_else(|| {
        if shapes.is_some() {
            FilterChipDefaults::tonal_filter_chip_colors()
        } else {
            FilterChipDefaults::filter_chip_colors()
        }
    });
    let horizontal_arrangement: Option<HorizontalArrangement> = horizontal_arrangement;
    let arrangement = horizontal_arrangement.unwrap_or_else(|| {
        if shapes.is_some() {
            FilterChipDefaults::horizontal_arrangement_for(
                leading_icon.is_some(),
                trailing_icon.is_some(),
            )
        } else {
            FilterChipDefaults::horizontal_arrangement()
        }
    });
    let border: ChipBorderChoice = border;
    let border = border.or_else(|| FilterChipDefaults::filter_chip_border(enabled, selected));
    selectable_with(
        selected,
        on_click,
        label,
        modifier,
        enabled,
        leading_icon,
        None,
        trailing_icon,
        shape,
        shapes,
        colors,
        elevation,
        border,
        arrangement,
        content_padding,
        interaction_source,
    );
}

/// A chip that's on or off, as Compose's `ElevatedFilterChip`: on a container, with a shadow.
/// `shapes` as `FilterChip`'s.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn ElevatedFilterChip(
    selected: bool,
    on_click: impl Fn() + 'static,
    label: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(crate::FilterChipDefaults::shape())] shape: Shape,
    #[default(None)] shapes: impl Into<Option<ChipShapes>>,
    #[default(None)] colors: impl Into<Option<SelectableChipColors>>,
    #[default(Some(crate::FilterChipDefaults::elevated_filter_chip_elevation()))] elevation: Option<
        SelectableChipElevation,
    >,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] horizontal_arrangement: impl Into<Option<HorizontalArrangement>>,
    #[default(crate::FilterChipDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    let shapes: Option<ChipShapes> = shapes;
    let colors: Option<SelectableChipColors> = colors;
    let colors = colors.unwrap_or_else(|| {
        if shapes.is_some() {
            FilterChipDefaults::tonal_elevated_filter_chip_colors()
        } else {
            FilterChipDefaults::elevated_filter_chip_colors()
        }
    });
    let horizontal_arrangement: Option<HorizontalArrangement> = horizontal_arrangement;
    let arrangement = horizontal_arrangement.unwrap_or_else(|| {
        if shapes.is_some() {
            FilterChipDefaults::horizontal_arrangement_for(
                leading_icon.is_some(),
                trailing_icon.is_some(),
            )
        } else {
            FilterChipDefaults::horizontal_arrangement()
        }
    });
    selectable_with(
        selected,
        on_click,
        label,
        modifier,
        enabled,
        leading_icon,
        None,
        trailing_icon,
        shape,
        shapes,
        colors,
        elevation,
        border,
        arrangement,
        content_padding,
        interaction_source,
    );
}

/// A chip for a piece of what's entered, as Compose's `InputChip`: selected as a filter chip is, its
/// `avatar` (a picture, clipped round and faded when disabled) taking the place of a leading icon.
/// `shapes` as `FilterChip`'s.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn InputChip(
    selected: bool,
    on_click: impl Fn() + 'static,
    label: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] avatar: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(crate::InputChipDefaults::shape())] shape: Shape,
    #[default(None)] shapes: impl Into<Option<ChipShapes>>,
    #[default(None)] colors: impl Into<Option<SelectableChipColors>>,
    #[default(Some(crate::InputChipDefaults::input_chip_elevation()))] elevation: Option<
        SelectableChipElevation,
    >,
    #[default(crate::ChipBorderChoice::Default)] border: impl Into<ChipBorderChoice>,
    #[default(None)] horizontal_arrangement: impl Into<Option<HorizontalArrangement>>,
    #[default(None)] content_padding: impl Into<Option<PaddingValues>>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    let shapes: Option<ChipShapes> = shapes;
    let colors: Option<SelectableChipColors> = colors;
    let colors = colors.unwrap_or_else(|| {
        if shapes.is_some() {
            InputChipDefaults::tonal_input_chip_colors()
        } else {
            InputChipDefaults::input_chip_colors()
        }
    });
    let (has_avatar, has_leading, has_trailing) =
        (avatar.is_some(), leading_icon.is_some(), trailing_icon.is_some());
    let horizontal_arrangement: Option<HorizontalArrangement> = horizontal_arrangement;
    let arrangement = horizontal_arrangement.unwrap_or_else(|| {
        if shapes.is_some() {
            InputChipDefaults::horizontal_arrangement_for(has_avatar, has_leading, has_trailing)
        } else {
            InputChipDefaults::horizontal_arrangement()
        }
    });
    let content_padding: Option<PaddingValues> = content_padding;
    let padding = content_padding.unwrap_or_else(|| {
        InputChipDefaults::content_padding(has_avatar, has_leading, has_trailing)
    });
    let border: ChipBorderChoice = border;
    let border = border.or_else(|| InputChipDefaults::input_chip_border(enabled, selected));
    let opacity = if enabled { 1.0 } else { 0.38 };
    let avatar = avatar.map(|avatar: Callback| {
        compose_runtime::callback(move || {
            let avatar = avatar.clone();
            Box(
                Modifier.graphics_layer(move |layer| {
                    layer.alpha = opacity;
                    layer.shape = compose_foundation::CIRCLE_SHAPE;
                    layer.clip = true;
                }),
                Alignment::CENTER,
                false,
                compose_runtime::callback(move |_| avatar()),
            );
        })
    });
    selectable_with(
        selected,
        on_click,
        label,
        modifier,
        enabled,
        leading_icon,
        avatar,
        trailing_icon,
        shape,
        shapes,
        colors,
        elevation,
        border,
        arrangement,
        padding,
        interaction_source,
    );
}

#[allow(clippy::too_many_arguments)]
fn selectable_with(
    selected: bool,
    on_click: Callback,
    label: Callback,
    modifier: Modifier,
    enabled: bool,
    leading: Option<Callback>,
    avatar: Option<Callback>,
    trailing: Option<Callback>,
    shape: Shape,
    shapes: Option<ChipShapes>,
    colors: SelectableChipColors,
    elevation: Option<SelectableChipElevation>,
    border: Option<BorderStroke>,
    arrangement: HorizontalArrangement,
    padding: PaddingValues,
    interaction_source: Option<MutableInteractionSource>,
) {
    let interaction_source = remembered_interaction_source(interaction_source);
    let shaped = shapes.is_some();
    // The pressed state's only read with `shapes`, as `SelectableChip`'s second overload.
    let shape = match shapes {
        Some(shapes) => shapes.for_state(
            selected,
            interaction_source.is_pressed(),
            MaterialTheme::motion_scheme().fast_spatial_spec(),
        ),
        None => shape,
    };
    selectable_chip(
        ChipParts {
            modifier,
            enabled,
            on_click,
            label,
            label_color: colors.label_color(enabled, selected),
            leading,
            avatar,
            trailing,
            leading_color: colors.leading_icon_content_color(enabled, selected),
            trailing_color: colors.trailing_icon_content_color(enabled, selected),
            shape,
            container: colors.container_color(enabled, selected),
            elevation,
            border,
            arrangement,
            padding,
            interaction_source,
        },
        selected,
        icon_specs(shaped),
    );
}

// ---- Badges ------------------------------------------------------------------------------------

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
        .background_with(container_color, pill.clone())
        .clip(pill);
    match content {
        Some(content) => {
            CompositionLocalProvider(
                (
                    LOCAL_CONTENT_COLOR.provides(content_color),
                    LOCAL_TEXT_STYLE.provides(typography.label_small.clone()),
                ),
                || {
                    let content = content.clone();
                    Box(
                        modifier.clone().padding((4.dp(), 0.dp())),
                        Alignment::CENTER,
                        false,
                        compose_runtime::callback(move |_| content()),
                    );
                },
            );
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
    // Each in a box of its own, as Compose's: a badge lambda showing nothing leaves an empty one.
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            let (badge, content) = (badge.clone(), content.clone());
            Box(Modifier, Alignment::CENTER, false, compose_runtime::callback(move |_| content()));
            Box(Modifier, Alignment::TOP_START, false, compose_runtime::callback(move |_| badge()));
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
        let content = scope.measure(*content, constraints);
        let badge = scope.measure(*badge, constraints.loosen());
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
