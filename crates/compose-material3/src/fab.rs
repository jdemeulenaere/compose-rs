//! Floating action buttons, as Compose's Material 3: `FloatingActionButton`,
//! `SmallFloatingActionButton`, `MediumFloatingActionButton`, `LargeFloatingActionButton` and the
//! extended ones, with `FloatingActionButtonDefaults`, `FloatingActionButtonElevation` and
//! `Modifier::animate_floating_action_button`.

use compose_animation::{
    Animatable, AnimatedVisibility, AnimationSpec, expand_horizontally_with, fade_in_with,
    fade_out_with, shrink_horizontally_with,
};
use compose_foundation::layout::{Arrangement, Box, Row, RowScope, Spacer};
use compose_runtime::{Callback, CompositionLocalProvider, composable};
use compose_ui::{
    Alignment, Color, Constraints, ContentDrawScope, Delegates, Density, Dp, DrawModifierNode,
    DrawScope, IntSize, LayoutModifierNode, Measurable, MeasureResult, MeasureScope, Modifier,
    ModifierNode, ModifierNodeElement, MutableInteractionSource, NodeContext, Offset, Rect, Role,
    Shape, Size, SizeSpec, TextStyle, UnitExt,
};

use crate::components::{Elevations, SurfaceClick, remembered_interaction_source, surface};
use crate::theme::{LOCAL_TEXT_STYLE, MaterialTheme};

/// A floating action button's elevation in each of its states, as Compose's
/// `FloatingActionButtonElevation`: its shadow, animated (`surface`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FloatingActionButtonElevation {
    pub default_elevation: Dp,
    pub pressed_elevation: Dp,
    pub focused_elevation: Dp,
    pub hovered_elevation: Dp,
}

impl FloatingActionButtonElevation {
    /// The shadow's elevation for the interaction `interaction_source` last began, as
    /// `shadowElevation(interactionSource)`.
    pub fn shadow_elevation(&self, interaction_source: &MutableInteractionSource) -> Dp {
        let elevations = Elevations {
            default: self.default_elevation,
            pressed: self.pressed_elevation,
            focused: self.focused_elevation,
            hovered: self.hovered_elevation,
            dragged: self.default_elevation,
            disabled: self.default_elevation,
        };
        crate::components::elevation_and_interaction(elevations, true, interaction_source).0
    }

    /// The tonal elevation: the resting one, as `tonalElevation()`.
    pub fn tonal_elevation(&self) -> Dp {
        self.default_elevation
    }
}

impl compose_runtime::ComposeArg for FloatingActionButtonElevation {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `FloatingActionButtonDefaults`.
pub struct FloatingActionButtonDefaults;

impl FloatingActionButtonDefaults {
    /// A large FAB's icon, as `LargeIconSize`.
    pub const LARGE_ICON_SIZE: Dp = Dp(36.0);
    /// As `MediumIconSize`.
    pub const MEDIUM_ICON_SIZE: Dp = Dp(28.0);

    /// The large shape (16 dp corners), as `shape`.
    pub fn shape() -> Shape {
        MaterialTheme::shapes().large.clone()
    }

    /// The medium shape (12 dp), as `smallShape`.
    pub fn small_shape() -> Shape {
        MaterialTheme::shapes().medium.clone()
    }

    /// The extra-large shape (28 dp), as `largeShape`.
    pub fn large_shape() -> Shape {
        MaterialTheme::shapes().extra_large.clone()
    }

    /// As `extendedFabShape`.
    pub fn extended_fab_shape() -> Shape {
        MaterialTheme::shapes().large.clone()
    }

    /// As `mediumShape`: the constant, not the theme's, as Compose's "TODO: update to use token".
    pub fn medium_shape() -> Shape {
        crate::theme::ShapeDefaults::LARGE_INCREASED
    }

    /// As `smallExtendedFabShape`.
    pub fn small_extended_fab_shape() -> Shape {
        MaterialTheme::shapes().large.clone()
    }

    /// As `mediumExtendedFabShape`: the constant, as `medium_shape`.
    pub fn medium_extended_fab_shape() -> Shape {
        crate::theme::ShapeDefaults::LARGE_INCREASED
    }

    /// As `largeExtendedFabShape`.
    pub fn large_extended_fab_shape() -> Shape {
        MaterialTheme::shapes().extra_large.clone()
    }

    /// The primary container, as `containerColor`.
    pub fn container_color() -> Color {
        MaterialTheme::color_scheme().primary_container
    }

    /// 6 dp, 8 dp hovered, as `elevation()`.
    pub fn elevation() -> FloatingActionButtonElevation {
        FloatingActionButtonElevation {
            default_elevation: Dp(6.0),
            pressed_elevation: Dp(6.0),
            focused_elevation: Dp(6.0),
            hovered_elevation: Dp(8.0),
        }
    }

    /// 1 dp, 3 dp hovered (`ElevationTokens.Level1`, `Level2`), as `loweredElevation()`.
    pub fn lowered_elevation() -> FloatingActionButtonElevation {
        FloatingActionButtonElevation {
            default_elevation: Dp(1.0),
            pressed_elevation: Dp(1.0),
            focused_elevation: Dp(1.0),
            hovered_elevation: Dp(3.0),
        }
    }

    /// Flat, for a FAB in a bottom app bar, as `bottomAppBarFabElevation()`.
    pub fn bottom_app_bar_fab_elevation() -> FloatingActionButtonElevation {
        FloatingActionButtonElevation {
            default_elevation: Dp(0.0),
            pressed_elevation: Dp(0.0),
            focused_elevation: Dp(0.0),
            hovered_elevation: Dp(0.0),
        }
    }
}

/// A FAB's look and how it goes.
#[derive(Clone)]
struct FabParts {
    on_click: Callback,
    modifier: Modifier,
    shape: Shape,
    container_color: Color,
    content_color: Color,
    elevation: FloatingActionButtonElevation,
    interaction_source: Option<MutableInteractionSource>,
}

/// The FABs' body, as Compose's `FloatingActionButton`: a clickable surface around its content,
/// centered in at least `min_width` by `min_height` unless the modifier gives another minimum, in
/// `text_style`.
fn fab(parts: FabParts, text_style: TextStyle, min_width: Dp, min_height: Dp, content: Callback) {
    let elevation = parts.elevation;
    let interaction_source = remembered_interaction_source(parts.interaction_source);
    let click = SurfaceClick {
        on_click: parts.on_click,
        enabled: true,
        role: Some(Role::Button),
        interaction_source,
        selected: None,
        checked: None,
        // The shadow follows the press, hover and focus, animated (`surface`).
        elevations: Some(Elevations {
            default: elevation.default_elevation,
            pressed: elevation.pressed_elevation,
            focused: elevation.focused_elevation,
            hovered: elevation.hovered_elevation,
            dragged: elevation.default_elevation,
            disabled: elevation.default_elevation,
        }),
    };
    surface(
        parts.modifier,
        parts.shape,
        parts.container_color,
        parts.content_color,
        elevation.tonal_elevation(),
        Dp(0.0),
        None,
        Some(click),
        compose_runtime::callback(move || {
            let content = content.clone();
            CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(text_style.clone()), || {
                let content = content.clone();
                Box(
                    Modifier.default_min_size(min_width, min_height),
                    Alignment::CENTER,
                    false,
                    compose_runtime::callback(move |_| content()),
                );
            });
        }),
    );
}

/// A FAB's own: label-large, at least 56 dp square, as Compose's `FloatingActionButton`.
fn plain_fab(parts: FabParts, content: Callback) {
    let label = MaterialTheme::typography().label_large.clone();
    fab(parts, label, FAB_SIZE, FAB_SIZE, content);
}

const FAB_SIZE: Dp = Dp(56.0);

/// What tells an extended FAB's sizes apart, as `SmallExtendedFab*` and the others.
struct ExtendedSize {
    minimum: Dp,
    start: Dp,
    end: Dp,
    icon_padding: Dp,
}

const SMALL_EXTENDED: ExtendedSize =
    ExtendedSize { minimum: Dp(56.0), start: Dp(16.0), end: Dp(16.0), icon_padding: Dp(8.0) };
const MEDIUM_EXTENDED: ExtendedSize =
    ExtendedSize { minimum: Dp(80.0), start: Dp(26.0), end: Dp(26.0), icon_padding: Dp(12.0) };
const LARGE_EXTENDED: ExtendedSize =
    ExtendedSize { minimum: Dp(96.0), start: Dp(28.0), end: Dp(28.0), icon_padding: Dp(16.0) };

/// An extended FAB of a size with a row of its own, as the overloads of `content`.
fn sized_extended_content(
    parts: FabParts,
    text_style: TextStyle,
    size: ExtendedSize,
    content: Callback<dyn Fn(RowScope)>,
) {
    let (start, end) = (size.start, size.end);
    fab(
        parts,
        text_style,
        size.minimum,
        size.minimum,
        compose_runtime::callback(move || {
            Row(
                Modifier.padding((start, Dp(0.0), end, Dp(0.0))),
                Arrangement::CENTER.into(),
                Alignment::CENTER_VERTICALLY,
                content.clone(),
            );
        }),
    );
}

/// An extended FAB of a size with an icon and a text, as the private
/// `ExtendedFloatingActionButton`: as wide as its row, down to a square as it collapses, the text
/// fading, its width and its alpha on one transition.
fn sized_extended(
    parts: FabParts,
    text_style: TextStyle,
    size: ExtendedSize,
    expanded: bool,
    text: Callback,
    icon: Callback,
) {
    let ExtendedSize { minimum, start, end, icon_padding } = size;
    fab(
        parts,
        text_style,
        Dp::UNSPECIFIED,
        Dp::UNSPECIFIED,
        compose_runtime::callback(move || {
            let (text, icon) = (text.clone(), icon.clone());
            let motion = MaterialTheme::motion_scheme();
            let transition = compose_animation::update_transition(
                if expanded { 1.0f32 } else { 0.0 },
                "expanded state",
            );
            let (fast_spatial, fast_effects) =
                (motion.fast_spatial_spec(), motion.fast_effects_spec());
            let width = transition.animate_float_with(
                move |_| fast_spatial.clone(),
                "ExtendedFabWidth",
                |state| *state,
            );
            let alpha = transition.animate_float_with(
                move |_| fast_effects.clone(),
                "ExtendedFabAlpha",
                |state| *state,
            );
            let fully_collapsed = transition.current_state() == 0.0 && !transition.is_running();
            Row(
                Modifier
                    .then_custom(ExpandingWidth { minimum, progress: width })
                    .size_in(SizeSpec::bounds(minimum, minimum, Dp::UNSPECIFIED, Dp::UNSPECIFIED))
                    .padding((start, Dp(0.0), end, Dp(0.0))),
                Arrangement::START,
                Alignment::CENTER_VERTICALLY,
                compose_runtime::callback(move |_| {
                    icon();
                    if !fully_collapsed {
                        let text = text.clone();
                        Row(
                            Modifier.clear_and_set_semantics(|_| {}).graphics_layer(move |layer| {
                                layer.alpha = alpha.get();
                            }),
                            Arrangement::START,
                            Alignment::TOP,
                            compose_runtime::callback(move |_| {
                                Spacer(Modifier.width(icon_padding));
                                text();
                            }),
                        );
                    }
                }),
            );
        }),
    );
}

/// `Modifier.layout` of the sized extended FABs: from the minimum to the full width of its row.
#[derive(Clone, Copy, PartialEq)]
struct ExpandingWidth {
    minimum: Dp,
    progress: compose_runtime::State<f32>,
}

impl ModifierNodeElement for ExpandingWidth {
    type Node = ExpandingWidth;
    fn create(&self) -> ExpandingWidth {
        *self
    }
    fn update(&self, node: &mut ExpandingWidth) {
        *node = *self;
    }
    fn same(&self, other: &Self) -> bool {
        self == other
    }
    fn name(&self) -> &'static str {
        "expandingWidth"
    }
}

impl ModifierNode for ExpandingWidth {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for ExpandingWidth {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let expanded = scope.max_intrinsic_width(content, constraints.max_height);
        let minimum = scope.density().round_to_px(self.minimum);
        let width = minimum + ((expanded - minimum) as f32 * self.progress.get()).round() as i32;
        let placeable = scope.measure(content, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(width, placeable.height)
    }
}

/// How `animate_floating_action_button` shows and hides a FAB.
pub trait FloatingActionButtonModifiers {
    /// Shows or hides a FAB with an animation, scaling it towards `alignment` and fading it, as
    /// `Modifier.animateFloatingActionButton(visible, alignment)`. Hidden it takes no room and no
    /// pointer.
    fn animate_floating_action_button(self, visible: bool, alignment: Alignment) -> Modifier;

    /// As `animateFloatingActionButton`, with its other parameters; `None` for the motion scheme's
    /// fast spatial and fast effects specs.
    fn animate_floating_action_button_with(
        self,
        visible: bool,
        alignment: Alignment,
        target_scale: f32,
        scale_animation_spec: Option<AnimationSpec>,
        alpha_animation_spec: Option<AnimationSpec>,
    ) -> Modifier;
}

/// `ShowHideTargetScale`.
const SHOW_HIDE_TARGET_SCALE: f32 = 0.2;

impl FloatingActionButtonModifiers for Modifier {
    fn animate_floating_action_button(self, visible: bool, alignment: Alignment) -> Modifier {
        self.animate_floating_action_button_with(
            visible,
            alignment,
            SHOW_HIDE_TARGET_SCALE,
            None,
            None,
        )
    }

    fn animate_floating_action_button_with(
        self,
        visible: bool,
        alignment: Alignment,
        target_scale: f32,
        scale_animation_spec: Option<AnimationSpec>,
        alpha_animation_spec: Option<AnimationSpec>,
    ) -> Modifier {
        self.then_custom(FabVisible {
            visible,
            alignment,
            target_scale,
            scale_animation_spec,
            alpha_animation_spec,
        })
    }
}

#[derive(Clone, PartialEq)]
struct FabVisible {
    visible: bool,
    alignment: Alignment,
    target_scale: f32,
    scale_animation_spec: Option<AnimationSpec>,
    alpha_animation_spec: Option<AnimationSpec>,
}

/// As Compose's `FabVisibleNode`: a layout node and what it draws, reading its animatables as it
/// draws; its scale and alpha around what's inside, as Compose's `drawLayer`. Hidden it doesn't
/// measure or place what's inside and takes no room, draws nothing, and isn't hit.
struct FabVisibleNode {
    params: FabVisible,
    scale: Animatable<f32>,
    alpha: Animatable<f32>,
    delegates: Delegates,
}

/// The room left around what's drawn for its shadow, as `layerInsetInt`.
const SHADOW_INSET: Dp = Dp(16.0);

impl ModifierNodeElement for FabVisible {
    type Node = FabVisibleNode;

    fn create(&self) -> FabVisibleNode {
        let at = if self.visible { 1.0 } else { 0.0 };
        FabVisibleNode {
            params: self.clone(),
            scale: Animatable::new(at),
            alpha: Animatable::new(at),
            delegates: Delegates::new(),
        }
    }

    fn update(&self, node: &mut FabVisibleNode) {
        node.params = self.clone();
        // A context to launch and read the motion scheme with: `on_update_with_context`.
        node.delegates.ask_for_context();
    }

    fn same(&self, other: &Self) -> bool {
        self == other
    }

    fn name(&self) -> &'static str {
        "animateFloatingActionButton"
    }
}

impl ModifierNode for FabVisibleNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }

    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }

    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }

    /// Hidden, measured at nothing: this clips everything inside out of a hit.
    fn hit_clip_at(&self, _size: Size, _density: Density) -> Option<Shape> {
        (self.alpha.value() == 0.0).then_some(Shape::Rectangle)
    }

    fn on_update_with_context(&mut self, context: &mut NodeContext<'_>) {
        let motion = context.current_value_of(&crate::motion::LOCAL_MOTION_SCHEME);
        let target = if self.params.visible { 1.0 } else { 0.0 };
        let scope = context.coroutine_scope();
        let scale_spec =
            self.params.scale_animation_spec.clone().unwrap_or_else(|| motion.fast_spatial_spec());
        let alpha_spec =
            self.params.alpha_animation_spec.clone().unwrap_or_else(|| motion.fast_effects_spec());
        let scale = self.scale.clone();
        scope.launch(async move {
            scale.animate_to(target, scale_spec).await;
        });
        let alpha = self.alpha.clone();
        scope.launch(async move {
            alpha.animate_to(target, alpha_spec).await;
        });
    }
}

impl LayoutModifierNode for FabVisibleNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        if self.alpha.value() == 0.0 {
            return scope.layout(0, 0);
        }
        let placeable = scope.measure(content, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

impl DrawModifierNode for FabVisibleNode {
    fn draw(&mut self, _scope: &mut DrawScope<'_>) {}

    fn draws_content(&self) -> bool {
        true
    }

    fn draw_with_content(&mut self, scope: &mut ContentDrawScope<'_, '_>) {
        let alpha = self.alpha.value();
        if alpha == 0.0 {
            return;
        }
        let from = self.params.target_scale;
        let scale = from + (1.0 - from) * self.scale.value();
        if alpha == 1.0 && scale == 1.0 {
            scope.draw_content();
            return;
        }
        let size = scope.size();
        // Towards its alignment: the pixel there, as `alignment.align(IntSize(1, 1), …)`.
        let pivot = self.params.alignment.align(
            IntSize::new(1, 1),
            IntSize::new(size.width as i32, size.height as i32),
            scope.layout_direction(),
        );
        let inset = scope.density().to_px(SHADOW_INSET);
        scope.save();
        scope.transform(
            Offset::new(pivot.x as f32, pivot.y as f32),
            (scale, scale),
            0.0,
            Offset::ZERO,
        );
        if alpha < 1.0 {
            let bounds = Rect::new(-inset, -inset, size.width + inset, size.height + inset);
            scope.save_layer_alpha(bounds, alpha);
        }
        scope.draw_content();
        if alpha < 1.0 {
            scope.restore();
        }
        scope.restore();
    }
}

/// A floating action button, as Compose's `FloatingActionButton`: 56 dp, the large shape, in the
/// primary container's colors, on a 6 dp shadow.
#[composable]
pub fn FloatingActionButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::FloatingActionButtonDefaults::shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    plain_fab(
        FabParts {
            on_click,
            modifier,
            shape,
            container_color,
            content_color,
            elevation,
            interaction_source,
        },
        content,
    );
}

/// A small floating action button, as Compose's `SmallFloatingActionButton`: 40 dp, the medium shape.
#[composable]
pub fn SmallFloatingActionButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::FloatingActionButtonDefaults::small_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    let modifier =
        modifier.size_in(SizeSpec::bounds(40.dp(), 40.dp(), Dp::UNSPECIFIED, Dp::UNSPECIFIED));
    plain_fab(
        FabParts {
            on_click,
            modifier,
            shape,
            container_color,
            content_color,
            elevation,
            interaction_source,
        },
        content,
    );
}

/// A medium floating action button, as Compose's `MediumFloatingActionButton`: 80 dp, the large increased shape (its icon `MEDIUM_ICON_SIZE`, 28 dp).
#[composable]
pub fn MediumFloatingActionButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::FloatingActionButtonDefaults::medium_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    let modifier =
        modifier.size_in(SizeSpec::bounds(80.dp(), 80.dp(), Dp::UNSPECIFIED, Dp::UNSPECIFIED));
    plain_fab(
        FabParts {
            on_click,
            modifier,
            shape,
            container_color,
            content_color,
            elevation,
            interaction_source,
        },
        content,
    );
}

/// A large floating action button, as Compose's `LargeFloatingActionButton`: 96 dp, the extra-large shape (its icon `LARGE_ICON_SIZE`, 36 dp).
#[composable]
pub fn LargeFloatingActionButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::FloatingActionButtonDefaults::large_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    let modifier =
        modifier.size_in(SizeSpec::bounds(96.dp(), 96.dp(), Dp::UNSPECIFIED, Dp::UNSPECIFIED));
    plain_fab(
        FabParts {
            on_click,
            modifier,
            shape,
            container_color,
            content_color,
            elevation,
            interaction_source,
        },
        content,
    );
}

/// A small extended floating action button with content of its own, as Compose's
/// `SmallExtendedFloatingActionButton(onClick, …, content)`.
#[composable]
pub fn SmallExtendedFloatingActionButtonContent(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::FloatingActionButtonDefaults::small_extended_fab_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    sized_extended_content(
        FabParts {
            on_click,
            modifier,
            shape,
            container_color,
            content_color,
            elevation,
            interaction_source,
        },
        MaterialTheme::typography().title_medium.clone(),
        SMALL_EXTENDED,
        content,
    );
}

/// A small extended floating action button, as Compose's
/// `SmallExtendedFloatingActionButton(text, icon, onClick, …, expanded)`: the icon and the text;
/// collapsed, the icon alone.
#[composable]
pub fn SmallExtendedFloatingActionButton(
    text: impl Fn() + 'static,
    icon: impl Fn() + 'static,
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] expanded: bool,
    #[default(crate::FloatingActionButtonDefaults::small_extended_fab_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    sized_extended(
        FabParts {
            on_click,
            modifier,
            shape,
            container_color,
            content_color,
            elevation,
            interaction_source,
        },
        MaterialTheme::typography().title_medium.clone(),
        SMALL_EXTENDED,
        expanded,
        text,
        icon,
    );
}

/// A medium extended floating action button with content of its own, as Compose's
/// `MediumExtendedFloatingActionButton(onClick, …, content)`.
#[composable]
pub fn MediumExtendedFloatingActionButtonContent(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::FloatingActionButtonDefaults::medium_extended_fab_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    sized_extended_content(
        FabParts {
            on_click,
            modifier,
            shape,
            container_color,
            content_color,
            elevation,
            interaction_source,
        },
        MaterialTheme::typography().title_large.clone(),
        MEDIUM_EXTENDED,
        content,
    );
}

/// A medium extended floating action button, as Compose's
/// `MediumExtendedFloatingActionButton(text, icon, onClick, …, expanded)`: the icon and the text;
/// collapsed, the icon alone.
#[composable]
pub fn MediumExtendedFloatingActionButton(
    text: impl Fn() + 'static,
    icon: impl Fn() + 'static,
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] expanded: bool,
    #[default(crate::FloatingActionButtonDefaults::medium_extended_fab_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    sized_extended(
        FabParts {
            on_click,
            modifier,
            shape,
            container_color,
            content_color,
            elevation,
            interaction_source,
        },
        MaterialTheme::typography().title_large.clone(),
        MEDIUM_EXTENDED,
        expanded,
        text,
        icon,
    );
}

/// A large extended floating action button with content of its own, as Compose's
/// `LargeExtendedFloatingActionButton(onClick, …, content)`.
#[composable]
pub fn LargeExtendedFloatingActionButtonContent(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::FloatingActionButtonDefaults::large_extended_fab_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    sized_extended_content(
        FabParts {
            on_click,
            modifier,
            shape,
            container_color,
            content_color,
            elevation,
            interaction_source,
        },
        MaterialTheme::typography().headline_small.clone(),
        LARGE_EXTENDED,
        content,
    );
}

/// A large extended floating action button, as Compose's
/// `LargeExtendedFloatingActionButton(text, icon, onClick, …, expanded)`: the icon and the text;
/// collapsed, the icon alone.
#[composable]
pub fn LargeExtendedFloatingActionButton(
    text: impl Fn() + 'static,
    icon: impl Fn() + 'static,
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] expanded: bool,
    #[default(crate::FloatingActionButtonDefaults::large_extended_fab_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    sized_extended(
        FabParts {
            on_click,
            modifier,
            shape,
            container_color,
            content_color,
            elevation,
            interaction_source,
        },
        MaterialTheme::typography().headline_small.clone(),
        LARGE_EXTENDED,
        expanded,
        text,
        icon,
    );
}

/// An extended floating action button with content of its own, as Compose's
/// `ExtendedFloatingActionButton(onClick, …, content)`: at least 80 dp wide, 20 dp in on each side.
#[composable]
pub fn ExtendedFloatingActionButtonContent(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::FloatingActionButtonDefaults::extended_fab_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    plain_fab(
        FabParts {
            on_click,
            modifier,
            shape,
            container_color,
            content_color,
            elevation,
            interaction_source,
        },
        compose_runtime::callback(move || {
            Row(
                Modifier
                    .size_in(SizeSpec::bounds(
                        80.dp(),
                        Dp::UNSPECIFIED,
                        Dp::UNSPECIFIED,
                        Dp::UNSPECIFIED,
                    ))
                    .padding((20.dp(), Dp(0.0))),
                Arrangement::CENTER.into(),
                Alignment::CENTER_VERTICALLY,
                content.clone(),
            );
        }),
    );
}

/// An extended floating action button, as Compose's `ExtendedFloatingActionButton(text, icon,
/// …, expanded)`: 56 dp high, the icon 16 dp in, then (while `expanded`) the text 12 dp after
/// it and 20 dp from the end, at least 80 dp wide; collapsed, the icon alone in a 56 dp square.
/// The text slides and fades in and out; it doesn't add to the button's semantics (as
/// Compose's, which clears it: give the icon a description).
#[composable]
pub fn ExtendedFloatingActionButton(
    text: impl Fn() + 'static,
    icon: impl Fn() + 'static,
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] expanded: bool,
    #[default(crate::FloatingActionButtonDefaults::extended_fab_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    let (start, end) = if expanded { (16.dp(), 20.dp()) } else { (Dp(0.0), Dp(0.0)) };
    let minimum_width = if expanded { 80.dp() } else { 56.dp() };
    plain_fab(
        FabParts {
            on_click,
            modifier,
            shape,
            container_color,
            content_color,
            elevation,
            interaction_source,
        },
        compose_runtime::callback(move || {
            let (text, icon) = (text.clone(), icon.clone());
            Row(
                Modifier
                    .size_in(SizeSpec::bounds(
                        minimum_width,
                        Dp::UNSPECIFIED,
                        Dp::UNSPECIFIED,
                        Dp::UNSPECIFIED,
                    ))
                    .padding((start, Dp(0.0), end, Dp(0.0))),
                if expanded { Arrangement::START } else { Arrangement::CENTER.into() },
                Alignment::CENTER_VERTICALLY,
                compose_runtime::callback(move |_| {
                    icon();
                    // The text 12 dp after the icon (the gap shrinking with it), not read.
                    let text = text.clone();
                    let motion = crate::theme::MaterialTheme::motion_scheme();
                    AnimatedVisibility(
                        expanded,
                        Modifier.clear_and_set_semantics(|_| {}),
                        // As `extendedFabExpandAnimation` and `extendedFabCollapseAnimation`.
                        Some(
                            fade_in_with(motion.default_effects_spec(), 0.0)
                                + expand_horizontally_with(
                                    motion.fast_spatial_spec(),
                                    Alignment::START,
                                    true,
                                    |_| 0,
                                ),
                        ),
                        Some(
                            fade_out_with(motion.fast_effects_spec(), 0.0)
                                + shrink_horizontally_with(
                                    motion.default_spatial_spec(),
                                    Alignment::START,
                                    true,
                                    |_| 0,
                                ),
                        ),
                        "ExtendedFloatingActionButton",
                        compose_runtime::callback(move |_| {
                            let text = text.clone();
                            Row(
                                Modifier,
                                Arrangement::START,
                                Alignment::TOP,
                                compose_runtime::callback(move |_| {
                                    Spacer(Modifier.width(12.dp()));
                                    text();
                                }),
                            );
                        }),
                    );
                }),
            );
        }),
    );
}
