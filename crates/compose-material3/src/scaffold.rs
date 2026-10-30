//! Screen structure, as Compose's: `Scaffold`, `TopAppBar`, `Card` and dividers.

use std::rc::Rc;

use compose_foundation::BorderStroke;
use compose_foundation::layout::{
    Box, Column, ColumnScope, MutableWindowInsets, WindowInsets, WindowInsetsModifiers,
};
use compose_runtime::{
    CompositionLocalProvider, MutableState, composable, mutable_state_of, remember,
};
use compose_ui::{
    Alignment, Color, Constraints, Dp, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier, MutableInteractionSource, PaddingValues, Shape, UnitExt,
};

use crate::theme::{LOCAL_CONTENT_COLOR, MaterialTheme};

// ---- Dividers --------------------------------------------------------------------------------

/// As Compose's `DividerDefaults` (`DividerTokens`).
pub struct DividerDefaults;

impl DividerDefaults {
    /// 1 dp, as `Thickness`.
    pub const THICKNESS: Dp = Dp(1.0);

    /// The outline variant, as `color`.
    pub fn color() -> Color {
        crate::MaterialTheme::color_scheme().outline_variant
    }
}

/// A horizontal line, as Compose's `HorizontalDivider`: as wide as allowed, `thickness` high, a
/// line of that width drawn through its middle (`Dp::HAIRLINE`: no height, a one-pixel line).
#[composable]
pub fn HorizontalDivider(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::DividerDefaults::THICKNESS)] thickness: Dp,
    #[default(crate::DividerDefaults::color())] color: Color,
) {
    compose_foundation::layout::Canvas(modifier.fill_max_width().height(thickness), move |scope| {
        let width = scope.density().to_px(thickness);
        let size = scope.size();
        scope.draw_line(
            compose_ui::Offset::new(0.0, width / 2.0),
            compose_ui::Offset::new(size.width, width / 2.0),
            compose_ui::Paint::stroke(color, width),
        );
    });
}

/// A vertical line, as Compose's `VerticalDivider`: as tall as allowed, `thickness` wide.
#[composable]
pub fn VerticalDivider(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::DividerDefaults::THICKNESS)] thickness: Dp,
    #[default(crate::DividerDefaults::color())] color: Color,
) {
    compose_foundation::layout::Canvas(modifier.fill_max_height().width(thickness), move |scope| {
        let width = scope.density().to_px(thickness);
        let size = scope.size();
        scope.draw_line(
            compose_ui::Offset::new(width / 2.0, 0.0),
            compose_ui::Offset::new(width / 2.0, size.height),
            compose_ui::Paint::stroke(color, width),
        );
    });
}

// ---- Card ------------------------------------------------------------------------------------

/// A card's colors, as Compose's `CardColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CardColors {
    pub container_color: Color,
    pub content_color: Color,
    pub disabled_container_color: Color,
    pub disabled_content_color: Color,
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
            // As Compose's: the surface variant at 38 % over the surface, the content at 38 %.
            disabled_container_color: crate::selection_blend(
                scheme.surface,
                scheme.surface_variant,
                0.38,
            ),
            disabled_content_color: scheme.on_surface.with_alpha(0.38),
        }
    }

    /// An elevated card's, as `CardDefaults.elevatedCardColors()`.
    pub fn elevated_card_colors() -> CardColors {
        let scheme = MaterialTheme::color_scheme();
        CardColors { container_color: scheme.surface_container_low, ..CardDefaults::card_colors() }
    }

    /// An outlined card's, as `CardDefaults.outlinedCardColors()`.
    pub fn outlined_card_colors() -> CardColors {
        let scheme = MaterialTheme::color_scheme();
        CardColors { container_color: scheme.surface, ..CardDefaults::card_colors() }
    }

    /// An outlined card's border, as `CardDefaults.outlinedCardBorder()`.
    pub fn outlined_card_border(enabled: bool) -> BorderStroke {
        let scheme = MaterialTheme::color_scheme();
        let color = if enabled {
            scheme.outline_variant
        } else {
            scheme.outline.with_alpha(0.12).composite_over(scheme.surface_container_low)
        };
        BorderStroke::new(Dp(1.0), color)
    }

    /// Flat, 1 dp hovered, 6 dp dragged, as `cardElevation()`.
    pub fn card_elevation() -> CardElevation {
        CardElevation {
            default_elevation: Dp(0.0),
            pressed_elevation: Dp(0.0),
            focused_elevation: Dp(0.0),
            hovered_elevation: Dp(1.0),
            dragged_elevation: Dp(6.0),
            disabled_elevation: Dp(0.0),
        }
    }

    /// 1 dp, 3 dp hovered, 8 dp dragged, as `elevatedCardElevation()`.
    pub fn elevated_card_elevation() -> CardElevation {
        CardElevation {
            default_elevation: Dp(1.0),
            pressed_elevation: Dp(1.0),
            focused_elevation: Dp(1.0),
            hovered_elevation: Dp(3.0),
            dragged_elevation: Dp(8.0),
            disabled_elevation: Dp(1.0),
        }
    }

    /// Flat, 6 dp dragged, as `outlinedCardElevation()`.
    pub fn outlined_card_elevation() -> CardElevation {
        CardElevation {
            default_elevation: Dp(0.0),
            pressed_elevation: Dp(0.0),
            focused_elevation: Dp(0.0),
            hovered_elevation: Dp(0.0),
            dragged_elevation: Dp(6.0),
            disabled_elevation: Dp(0.0),
        }
    }

    pub fn shape() -> Shape {
        MaterialTheme::shapes().medium.clone()
    }

    /// As `elevatedShape`.
    pub fn elevated_shape() -> Shape {
        MaterialTheme::shapes().medium.clone()
    }

    /// As `outlinedShape`.
    pub fn outlined_shape() -> Shape {
        MaterialTheme::shapes().medium.clone()
    }
}

/// A card's elevation in each of its states, as Compose's `CardElevation`: the shadow follows a
/// clickable card's press, focus and hover (snapping, where Compose animates; nothing drags a
/// card here, so `dragged_elevation` isn't used yet), the tonal elevation is the resting one.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CardElevation {
    pub default_elevation: Dp,
    pub pressed_elevation: Dp,
    pub focused_elevation: Dp,
    pub hovered_elevation: Dp,
    pub dragged_elevation: Dp,
    pub disabled_elevation: Dp,
}

impl CardElevation {
    /// The tonal elevation, as `tonalElevation(enabled)`.
    pub fn tonal_elevation(&self, enabled: bool) -> Dp {
        if enabled { self.default_elevation } else { self.disabled_elevation }
    }

    /// The shadow's elevation, as `shadowElevation(enabled, interactionSource)`: a card that
    /// isn't clickable (no `interaction_source`) rests at its default.
    pub fn shadow_elevation(
        &self,
        enabled: bool,
        interaction_source: Option<&MutableInteractionSource>,
    ) -> Dp {
        match interaction_source {
            Some(interaction_source) => crate::button::interaction_elevation(
                enabled,
                self.default_elevation,
                self.pressed_elevation,
                self.focused_elevation,
                self.hovered_elevation,
                self.disabled_elevation,
                interaction_source,
            ),
            None => self.default_elevation,
        }
    }
}

impl compose_runtime::ComposeArg for CardElevation {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A filled card, as Compose's `Card`: its content in a column on a surface of the medium
/// shape; clickable when `on_click` is given (Compose's `Card(onClick)` overload), its shadow
/// then following its press, focus and hover.
#[composable]
pub fn Card(
    #[default(None)] on_click: Option<impl Fn() + 'static>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::CardDefaults::shape())] shape: Shape,
    #[default(crate::CardDefaults::card_colors())] colors: CardColors,
    #[default(crate::CardDefaults::card_elevation())] elevation: CardElevation,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(ColumnScope) + 'static,
) {
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    let (container, content_color) = if enabled {
        (colors.container_color, colors.content_color)
    } else {
        (colors.disabled_container_color, colors.disabled_content_color)
    };
    // Clickable, the shadow follows the press and hover while drawing; else it rests.
    let shadow = if on_click.is_some() { Dp(0.0) } else { elevation.default_elevation };
    let click = on_click.map(|on_click| crate::components::SurfaceClick {
        on_click,
        enabled,
        role: None,
        interaction_source,
        selected: None,
        checked: None,
        elevations: Some(crate::components::Elevations {
            default: elevation.default_elevation,
            pressed: elevation.pressed_elevation,
            focused: elevation.focused_elevation,
            hovered: elevation.hovered_elevation,
            disabled: elevation.disabled_elevation,
        }),
    });
    crate::components::surface(
        modifier,
        shape,
        container,
        content_color,
        elevation.tonal_elevation(enabled),
        shadow,
        border,
        click,
        compose_runtime::callback(move || {
            Column(
                Modifier,
                compose_foundation::layout::Arrangement::TOP,
                compose_ui::Alignment::START,
                content.clone(),
            );
        }),
    );
}

/// A card raised on a 1 dp shadow, as Compose's `ElevatedCard`.
#[composable]
pub fn ElevatedCard(
    #[default(None)] on_click: Option<impl Fn() + 'static>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::CardDefaults::elevated_shape())] shape: Shape,
    #[default(crate::CardDefaults::elevated_card_colors())] colors: CardColors,
    #[default(crate::CardDefaults::elevated_card_elevation())] elevation: CardElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(ColumnScope) + 'static,
) {
    Card(on_click, modifier, enabled, shape, colors, elevation, None, interaction_source, content);
}

/// A card with an outline, as Compose's `OutlinedCard`; `border` `None` is the default
/// outline, `outlinedCardBorder(enabled)`.
#[composable]
pub fn OutlinedCard(
    #[default(None)] on_click: Option<impl Fn() + 'static>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::CardDefaults::outlined_shape())] shape: Shape,
    #[default(crate::CardDefaults::outlined_card_colors())] colors: CardColors,
    #[default(crate::CardDefaults::outlined_card_elevation())] elevation: CardElevation,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(ColumnScope) + 'static,
) {
    let border = border.or_else(|| Some(CardDefaults::outlined_card_border(enabled)));
    Card(
        on_click,
        modifier,
        enabled,
        shape,
        colors,
        elevation,
        border,
        interaction_source,
        content,
    );
}

// ---- Scaffold --------------------------------------------------------------------------------

/// Where a `Scaffold` places its floating action button, as Compose's `FabPosition`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FabPosition {
    /// At the start, above the bottom bar.
    Start,
    /// Centered, above the bottom bar.
    Center,
    /// At the end, above the bottom bar.
    End,
    /// At the end, over the bottom bar.
    EndOverlay,
}

impl compose_runtime::ComposeArg for FabPosition {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `ScaffoldDefaults`.
pub struct ScaffoldDefaults;

impl ScaffoldDefaults {
    /// The insets the content keeps clear of, as `contentWindowInsets`: the system bars and a
    /// cutout.
    pub fn content_window_insets() -> WindowInsets {
        system_bars_for_visual_components()
    }
}

/// What Material's components keep clear of by default, as Compose's internal
/// `systemBarsForVisualComponents` on Android.
pub(crate) fn system_bars_for_visual_components() -> WindowInsets {
    WindowInsets::system_bars().union(WindowInsets::display_cutout())
}

/// The basic screen layout, as Compose's `Scaffold`: the top bar, the content (given the padding
/// that keeps it clear of the bars, or of `content_window_insets` where there's no bar), the
/// bottom bar, and the floating action button 16 dp from the edges at
/// `floating_action_button_position`, above the bottom bar (over it for `EndOverlay`); the
/// snackbar host above all of them, centered.
///
/// Compose measures the bars before composing the content (subcomposition); here the content
/// reads the padding as state its measurement writes, composed again within the same frame.
#[composable]
pub fn Scaffold(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(|| {})] top_bar: impl Fn() + 'static,
    #[default(|| {})] bottom_bar: impl Fn() + 'static,
    #[default(|| {})] snackbar_host: impl Fn() + 'static,
    #[default(|| {})] floating_action_button: impl Fn() + 'static,
    #[default(crate::FabPosition::End)] floating_action_button_position: FabPosition,
    #[default(crate::MaterialTheme::color_scheme().background)] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::ScaffoldDefaults::content_window_insets())]
    content_window_insets: WindowInsets,
    content: impl Fn(PaddingValues) + 'static,
) {
    let content_color =
        content_color.or(MaterialTheme::color_scheme().content_color_for(container_color));
    let padding = remember(|| mutable_state_of(PaddingValues::ZERO));
    // The insets less what's consumed around it, as Compose's: read while measuring, so a change
    // measures it again and composes only the content that reads the padding.
    let (safe_insets, consumed) =
        compose_runtime::remember_keyed(content_window_insets.clone(), || {
            let safe_insets = MutableWindowInsets::new(content_window_insets.clone());
            let (target, insets) = (safe_insets.clone(), content_window_insets.clone());
            let consumed =
                Modifier.on_consumed_window_insets_changed(move |consumed: WindowInsets| {
                    target.set_insets(insets.clone().exclude(consumed));
                });
            (safe_insets, consumed)
        });
    let policy = compose_runtime::remember_keyed(
        (floating_action_button_position, safe_insets.clone()),
        || {
            Rc::new(ScaffoldLayout {
                padding,
                fab_position: floating_action_button_position,
                insets: safe_insets.as_window_insets(),
            }) as Rc<dyn MeasurePolicy>
        },
    );
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(content_color), || {
        let (top_bar, bottom_bar, snackbar_host, fab, content) = (
            top_bar.clone(),
            bottom_bar.clone(),
            snackbar_host.clone(),
            floating_action_button.clone(),
            content.clone(),
        );
        Layout(
            modifier.then(consumed.clone()).background(container_color),
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
                slot(snackbar_host.clone());
            }),
        );
    });
}

/// Children: the top bar, the content, the bottom bar, the floating action button, the
/// snackbar host (placed last: over the rest).
struct ScaffoldLayout {
    padding: MutableState<PaddingValues>,
    fab_position: FabPosition,
    insets: WindowInsets,
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
        let [top, content, bottom, fab, snackbar] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let loose = constraints.loosen();
        // The snackbar and the button keep clear of the insets, as Compose's.
        let density = scope.density();
        let direction = scope.layout_direction();
        let (left_inset, right_inset) =
            (self.insets.get_left(&density, direction), self.insets.get_right(&density, direction));
        let bottom_inset = self.insets.get_bottom(&density);
        let (start_inset, end_inset) = match direction {
            compose_ui::LayoutDirection::Ltr => (left_inset, right_inset),
            compose_ui::LayoutDirection::Rtl => (right_inset, left_inset),
        };
        let inside_insets = loose.offset(-left_inset - right_inset, -bottom_inset);
        let top = scope.measure(*top, loose);
        let bottom = scope.measure(*bottom, loose);
        let fab = scope.measure(*fab, inside_insets);
        let snackbar = scope.measure(*snackbar, inside_insets);
        // The content's padding: the bars' heights, or the insets where there's no bar, as
        // Compose's. Written as state the content reads, which composes it again (within the
        // frame) when they change.
        let insets = self.insets.as_padding_values_with(&density, direction);
        let to_dp = |pixels: i32| Dp(pixels as f32 / density.density);
        let padding = PaddingValues::new(
            insets.start,
            if top.height == 0 { insets.top } else { to_dp(top.height) },
            insets.end,
            if bottom.height == 0 { insets.bottom } else { to_dp(bottom.height) },
        );
        if self.padding.get() != padding {
            self.padding.set(padding);
        }
        let content = scope.measure(*content, Constraints::fixed(width, height));
        scope.place_relative(&content, 0, 0);
        scope.place_relative(&top, 0, 0);
        scope.place_relative(&bottom, 0, height - bottom.height);
        // The FAB 16 dp from the edges, above the bottom bar (over it for `EndOverlay`: above
        // the bottom inset then), and the snackbar above both, centered, as Compose's
        // `ScaffoldLayout`.
        let mut above = if bottom.height == 0 { bottom_inset } else { bottom.height };
        if fab.width > 0 && fab.height > 0 {
            let margin = density.round_to_px(16.dp());
            let x = match self.fab_position {
                FabPosition::Start => margin + start_inset,
                FabPosition::Center => (width - fab.width + start_inset - end_inset) / 2,
                FabPosition::End | FabPosition::EndOverlay => {
                    width - fab.width - margin - end_inset
                }
            };
            let from_bottom = if bottom.height == 0 || self.fab_position == FabPosition::EndOverlay
            {
                fab.height + margin + bottom_inset
            } else {
                bottom.height + fab.height + margin
            };
            scope.place_relative(&fab, x, height - from_bottom);
            above = from_bottom;
        }
        if snackbar.height > 0 {
            scope.place_relative(
                &snackbar,
                (width - snackbar.width + start_inset - end_inset) / 2,
                height - above - snackbar.height,
            );
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "Scaffold"
    }

    // As Compose's, built on `SubcomposeLayout`: no intrinsics (D1207).
    fn min_intrinsic_width(
        &self,
        _: &mut compose_ui::IntrinsicMeasureScope<'_>,
        _: &[compose_ui::IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", compose_ui::platform::NO_INTRINSICS_MESSAGE)
    }

    fn min_intrinsic_height(
        &self,
        _: &mut compose_ui::IntrinsicMeasureScope<'_>,
        _: &[compose_ui::IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", compose_ui::platform::NO_INTRINSICS_MESSAGE)
    }

    fn max_intrinsic_width(
        &self,
        _: &mut compose_ui::IntrinsicMeasureScope<'_>,
        _: &[compose_ui::IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", compose_ui::platform::NO_INTRINSICS_MESSAGE)
    }

    fn max_intrinsic_height(
        &self,
        _: &mut compose_ui::IntrinsicMeasureScope<'_>,
        _: &[compose_ui::IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", compose_ui::platform::NO_INTRINSICS_MESSAGE)
    }
}
