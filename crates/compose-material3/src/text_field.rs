//! `TextField` and `OutlinedTextField`, as Compose's Material 3: a `BasicTextField` decorated
//! with a container (filled) or an outline, a label that rests in the field and floats up
//! (animated) when it's focused or has text, and a placeholder.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::Animatable;
use compose_foundation::layout::Box;
use compose_foundation::{BasicTextField, KeyboardActions, KeyboardOptions, VisualTransformation};
use compose_runtime::{Callback, CompositionLocalProvider, composable, mutable_state_of, remember};
use compose_ui::{
    Alignment, Color, Constraints, Dp, DrawModifierNode, HorizontalAlignment, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement,
    MutableInteractionSource, NodeContext, PaddingValues, Paint, Path, Shape, SizeSpec, TextStyle,
    UnitExt,
};

use crate::components::{TextFieldColors, TextFieldDefaults};
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

/// As Compose's `OutlinedTextFieldDefaults`.
pub struct OutlinedTextFieldDefaults;

impl OutlinedTextFieldDefaults {
    pub const MIN_WIDTH: Dp = Dp(280.0);
    pub const MIN_HEIGHT: Dp = Dp(56.0);
    /// The outline's width, as `UnfocusedBorderThickness` and `FocusedBorderThickness`.
    pub const UNFOCUSED_BORDER_THICKNESS: Dp = Dp(1.0);
    pub const FOCUSED_BORDER_THICKNESS: Dp = Dp(2.0);

    /// As `contentPaddingWithLabel()`: the same as a filled field's.
    pub const fn content_padding_with_label() -> PaddingValues {
        TextFieldDefaults::content_padding_with_label()
    }

    /// As `contentPaddingWithoutLabel()`.
    pub const fn content_padding_without_label() -> PaddingValues {
        TextFieldDefaults::content_padding_without_label()
    }

    /// The expressive rounded shape, as `roundedShape`: `CornerMedium`.
    pub fn rounded_shape() -> Shape {
        MaterialTheme::shapes().medium.clone()
    }

    /// As `tonalColors()`: no container, the outline in `outline_variant`.
    pub fn tonal_colors() -> TextFieldColors {
        let scheme = MaterialTheme::color_scheme();
        TextFieldColors {
            container_color: Color::TRANSPARENT,
            focused_container_color: Color::TRANSPARENT,
            error_container_color: Color::TRANSPARENT,
            disabled_container_color: Color::TRANSPARENT,
            focused_indicator_color: scheme.outline_variant,
            unfocused_indicator_color: scheme.outline_variant,
            disabled_indicator_color: scheme.outline_variant,
            error_indicator_color: scheme.error,
            ..crate::components::tonal_text_colors()
        }
    }

    /// From the theme, as `OutlinedTextFieldDefaults.colors()`: no container, the indicator
    /// colors for the outline.
    pub fn colors() -> TextFieldColors {
        let scheme = MaterialTheme::color_scheme();
        TextFieldColors {
            container_color: Color::TRANSPARENT,
            focused_container_color: Color::TRANSPARENT,
            error_container_color: Color::TRANSPARENT,
            disabled_container_color: Color::TRANSPARENT,
            unfocused_indicator_color: scheme.outline,
            disabled_indicator_color: scheme.on_surface.with_alpha(0.12),
            ..TextFieldDefaults::colors()
        }
    }

    /// The outline's shape, as `OutlinedTextFieldDefaults.shape` (extra small).
    pub fn shape() -> Shape {
        MaterialTheme::shapes().extra_small.clone()
    }
}

/// Where a field's label is, as Compose's `TextFieldLabelPosition`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TextFieldLabelPosition {
    /// In the container, at the top when minimized: the default of `TextField`.
    Inside {
        is_always_minimized: bool,
        minimized_alignment: HorizontalAlignment,
        expanded_alignment: HorizontalAlignment,
    },
    /// On the container's top edge when minimized, cut into it: the default of
    /// `OutlinedTextField`.
    Cutout {
        is_always_minimized: bool,
        minimized_alignment: HorizontalAlignment,
        expanded_alignment: HorizontalAlignment,
    },
    /// Above the container, always minimized.
    Above { alignment: HorizontalAlignment },
}

impl TextFieldLabelPosition {
    /// `Inside()`.
    pub const fn inside() -> TextFieldLabelPosition {
        TextFieldLabelPosition::Inside {
            is_always_minimized: false,
            minimized_alignment: Alignment::START,
            expanded_alignment: Alignment::START,
        }
    }

    /// `Cutout()`.
    pub const fn cutout() -> TextFieldLabelPosition {
        TextFieldLabelPosition::Cutout {
            is_always_minimized: false,
            minimized_alignment: Alignment::START,
            expanded_alignment: Alignment::START,
        }
    }

    /// `Above()`.
    pub const fn above() -> TextFieldLabelPosition {
        TextFieldLabelPosition::Above { alignment: Alignment::START }
    }

    /// Whether it can rest at the size of the text, as `showExpandedLabel`.
    fn shows_expanded(self) -> bool {
        match self {
            TextFieldLabelPosition::Inside { is_always_minimized, .. }
            | TextFieldLabelPosition::Cutout { is_always_minimized, .. } => !is_always_minimized,
            TextFieldLabelPosition::Above { .. } => false,
        }
    }

    fn minimized_alignment(self) -> HorizontalAlignment {
        match self {
            TextFieldLabelPosition::Inside { minimized_alignment, .. }
            | TextFieldLabelPosition::Cutout { minimized_alignment, .. } => minimized_alignment,
            TextFieldLabelPosition::Above { alignment } => alignment,
        }
    }

    fn expanded_alignment(self) -> HorizontalAlignment {
        match self {
            TextFieldLabelPosition::Inside { expanded_alignment, .. }
            | TextFieldLabelPosition::Cutout { expanded_alignment, .. } => expanded_alignment,
            TextFieldLabelPosition::Above { alignment } => alignment,
        }
    }

    fn is_cutout(self) -> bool {
        matches!(self, TextFieldLabelPosition::Cutout { .. })
    }

    fn is_above(self) -> bool {
        matches!(self, TextFieldLabelPosition::Above { .. })
    }

    fn is_inside(self) -> bool {
        matches!(self, TextFieldLabelPosition::Inside { .. })
    }
}

impl compose_runtime::ComposeArg for TextFieldLabelPosition {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl std::fmt::Display for TextFieldLabelPosition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (name, minimized, expanded) = match *self {
            TextFieldLabelPosition::Above { alignment } => {
                return write!(formatter, "Above(alignment=Horizontal(bias={:?}))", alignment.bias);
            }
            TextFieldLabelPosition::Inside {
                is_always_minimized,
                minimized_alignment,
                expanded_alignment,
            } => (("Inside", is_always_minimized), minimized_alignment, expanded_alignment),
            TextFieldLabelPosition::Cutout {
                is_always_minimized,
                minimized_alignment,
                expanded_alignment,
            } => (("Cutout", is_always_minimized), minimized_alignment, expanded_alignment),
        };
        write!(
            formatter,
            "{}(isAlwaysMinimized={}, minimizedAlignment=Horizontal(bias={:?}), \
             expandedAlignment=Horizontal(bias={:?}))",
            name.0, name.1, minimized.bias, expanded.bias
        )
    }
}

/// What a label knows of its field, as Compose's `TextFieldLabelScope`, here from
/// [`LOCAL_TEXT_FIELD_LABEL_SCOPE`], `label` being a plain closure.
#[derive(Clone, Copy, PartialEq, Default)]
pub struct TextFieldLabelScope {
    progress: Option<compose_runtime::State<f32>>,
}

impl TextFieldLabelScope {
    /// 0 at the size of the text, 1 minimized, as `labelMinimizedProgress` (a state read: read it
    /// in a layer or a draw, so a frame of it composes nothing).
    pub fn label_minimized_progress(&self) -> f32 {
        self.progress.map_or(1.0, |progress| progress.get())
    }
}

impl compose_runtime::ComposeArg for TextFieldLabelScope {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The scope of the label being composed.
pub static LOCAL_TEXT_FIELD_LABEL_SCOPE: compose_runtime::CompositionLocal<TextFieldLabelScope> =
    compose_runtime::CompositionLocal::new(
        "LocalTextFieldLabelScope",
        TextFieldLabelScope::default,
    );

/// The label, its style and color read here, so a frame of them composes this alone, as
/// `DecoratedLabel`: from the body-large text's style to the small one's as it floats.
#[composable]
#[allow(non_snake_case)]
fn FieldLabel(progress: compose_runtime::State<f32>, color: Color, focused: bool, label: Callback) {
    let motion = MaterialTheme::motion_scheme();
    let typography = MaterialTheme::typography();
    let (large, small) = (&typography.body_large, &typography.body_small);
    let color = compose_animation::animate_color_as_state(
        color,
        motion.fast_effects_spec(),
        "LabelContentColor",
        None,
    )
    .get();
    let mut style = TextStyle::lerp(large, small, progress.get());
    // One of the two with a color of its own: that color, as `labelTextStyleColor`, focused the
    // small one's, else the large one's, a missing one the label's.
    if large.color.is_specified() != small.color.is_specified() {
        let target = if focused { small.color } else { large.color };
        style.color = compose_animation::animate_color_as_state(
            target.or(color),
            motion.fast_effects_spec(),
            "LabelTextStyleColor",
            None,
        )
        .get();
    }
    Box!({
        CompositionLocalProvider(
            (
                LOCAL_CONTENT_COLOR.provides(color),
                LOCAL_TEXT_STYLE.provides(style.clone()),
                LOCAL_TEXT_FIELD_LABEL_SCOPE
                    .provides(TextFieldLabelScope { progress: Some(progress) }),
            ),
            || LabelContent(label.clone()),
        );
    });
}

/// The container's shape less the cut for a label on its edge, as `outlineCutout` on a filled
/// field's container.
fn draw_cut_container(
    scope: &mut compose_ui::DrawScope<'_>,
    shape: &Shape,
    color: Color,
    start: f32,
    shown: f32,
    half_height: f32,
) {
    let cut = label_room(scope, start, shown, half_height);
    let whole = Path::new();
    crate::tooltip::add_outline(
        &whole,
        shape.outline_in(scope.size(), &scope.density(), scope.layout_direction()),
    );
    let path = Path::combine(compose_ui::PathOperation::Difference, &whole, &cut);
    scope.draw_path(&path, Paint::fill(color));
}

/// The room a label on the edge takes from its container, 4 dp either side of it.
fn label_room(scope: &compose_ui::DrawScope<'_>, start: f32, shown: f32, half_height: f32) -> Path {
    let inner = scope.density().to_px(4.dp());
    let rtl = scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
    let left = if rtl { scope.size().width - start - shown } else { start };
    let cut = Path::new();
    cut.add_rect(compose_ui::Rect::new(
        left - inner,
        -half_height,
        left + shown + inner,
        half_height,
    ));
    cut
}

/// All a container of its own can draw in but the room for the label: as `ClipOp.Difference`.
fn cut_container_path(
    scope: &compose_ui::DrawScope<'_>,
    start: f32,
    shown: f32,
    half_height: f32,
) -> Path {
    let cut = label_room(scope, start, shown, half_height);
    let size = scope.size();
    let around = Path::new();
    // Past the bounds: a drawing outside them isn't cut by Compose's difference either.
    let (far_x, far_y) = (size.width.max(size.height), size.width.max(size.height));
    around.add_rect(compose_ui::Rect::new(-far_x, -far_y, size.width + far_x, size.height + far_y));
    Path::combine(compose_ui::PathOperation::Difference, &around, &cut)
}

/// The label's content, skipped while a frame of the progress only changes its style, as the lambda
/// Compose's `Decoration` is given.
#[composable]
#[allow(non_snake_case)]
fn LabelContent(label: Callback) {
    label();
}

/// As `EasingEmphasizedAccelerateCubicBezier`.
const LABEL_HEIGHT_EASING: compose_animation::Easing =
    compose_animation::Easing::cubic_bezier(0.3, 0.0, 0.8, 0.15);

/// The paddings by what a label takes: inside, 8 dp above and below; else 16, as
/// `defaultContentPadding`.
fn default_content_padding(has_label: bool, position: TextFieldLabelPosition) -> PaddingValues {
    if has_label && position.is_inside() {
        TextFieldDefaults::content_padding_with_label()
    } else {
        TextFieldDefaults::content_padding_without_label()
    }
}

/// A filled field's decoration around a [`BasicTextField`]'s inner field, as
/// `TextFieldDefaults.DecorationBox`: give it to its `decoration_box` for a custom field, with the
/// same `value`, `enabled`, `single_line`, `visual_transformation` and `interaction_source`.
/// `content_padding` by default goes by the label (`content_padding_with_label` with one inside,
/// `content_padding_without_label` else); `container`, by default as [`TextFieldContainer`].
/// As `decorator` too: without `TextFieldState`, the value is the state.
#[composable]
#[allow(non_snake_case)]
pub fn TextFieldDecorationBox(
    value: String,
    inner_text_field: impl Fn() + 'static,
    enabled: bool,
    single_line: bool,
    visual_transformation: impl Into<VisualTransformation>,
    interaction_source: MutableInteractionSource,
    #[default(false)] is_error: bool,
    #[default(crate::TextFieldLabelPosition::inside())] label_position: TextFieldLabelPosition,
    #[default(None)] label: Option<impl Fn() + 'static>,
    #[default(None)] placeholder: Option<impl Fn() + 'static>,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(None)] prefix: Option<impl Fn() + 'static>,
    #[default(None)] suffix: Option<impl Fn() + 'static>,
    #[default(None)] supporting_text: Option<impl Fn() + 'static>,
    #[default(crate::TextFieldDefaults::shape())] shape: Shape,
    #[default(crate::TextFieldDefaults::colors())] colors: TextFieldColors,
    #[default(None)] content_padding: Option<PaddingValues>,
    #[default(None)] container: Option<impl Fn() + 'static>,
) {
    standing_decoration(
        Variant::Filled,
        value,
        inner_text_field,
        (enabled, is_error, single_line),
        visual_transformation,
        interaction_source,
        label_position,
        Slots { label, placeholder, leading_icon, trailing_icon, prefix, suffix, supporting_text },
        shape,
        colors,
        content_padding,
        container,
    );
}

/// An outlined field's decoration, as `OutlinedTextFieldDefaults.DecorationBox`: see
/// [`TextFieldDecorationBox`], `container` by default as [`OutlinedTextFieldContainer`].
#[composable]
#[allow(non_snake_case)]
pub fn OutlinedTextFieldDecorationBox(
    value: String,
    inner_text_field: impl Fn() + 'static,
    enabled: bool,
    single_line: bool,
    visual_transformation: impl Into<VisualTransformation>,
    interaction_source: MutableInteractionSource,
    #[default(false)] is_error: bool,
    #[default(crate::TextFieldLabelPosition::cutout())] label_position: TextFieldLabelPosition,
    #[default(None)] label: Option<impl Fn() + 'static>,
    #[default(None)] placeholder: Option<impl Fn() + 'static>,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(None)] prefix: Option<impl Fn() + 'static>,
    #[default(None)] suffix: Option<impl Fn() + 'static>,
    #[default(None)] supporting_text: Option<impl Fn() + 'static>,
    #[default(crate::OutlinedTextFieldDefaults::colors())] colors: TextFieldColors,
    #[default(None)] content_padding: Option<PaddingValues>,
    #[default(None)] container: Option<impl Fn() + 'static>,
) {
    standing_decoration(
        Variant::Outlined,
        value,
        inner_text_field,
        (enabled, is_error, single_line),
        visual_transformation,
        interaction_source,
        label_position,
        Slots { label, placeholder, leading_icon, trailing_icon, prefix, suffix, supporting_text },
        OutlinedTextFieldDefaults::shape(),
        colors,
        content_padding,
        container,
    );
}

/// What the two decoration boxes have in common, as `CommonDecorationBox`: focused by the source.
#[allow(clippy::too_many_arguments)]
fn standing_decoration(
    variant: Variant,
    value: String,
    inner_text_field: Callback,
    (enabled, is_error, single_line): (bool, bool, bool),
    visual_transformation: VisualTransformation,
    interaction_source: MutableInteractionSource,
    label_position: TextFieldLabelPosition,
    slots: Slots,
    shape: Shape,
    colors: TextFieldColors,
    content_padding: Option<PaddingValues>,
    container: Option<Callback>,
) {
    let is_focused = interaction_source.collect_is_focused_as_state().get();
    let empty = visual_transformation
        .filter(&value)
        .map_or(value.is_empty(), |transformed| transformed.text.is_empty());
    let content_padding =
        content_padding.unwrap_or(default_content_padding(slots.label.is_some(), label_position));
    decoration_box(
        DecorationParts {
            variant,
            empty,
            is_focused,
            focus_modifier: Modifier,
            min_size: false,
            enabled,
            is_error,
            single_line,
            label_position,
            slots,
            colors,
            content_padding,
            shape,
            container,
        },
        inner_text_field,
    );
}

/// A filled field's container, as `TextFieldDefaults.Container`: its color animated, and an
/// [`indicator_line`](TextFieldModifiers::indicator_line).
#[composable]
#[allow(non_snake_case)]
pub fn TextFieldContainer(
    enabled: bool,
    is_error: bool,
    interaction_source: MutableInteractionSource,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::TextFieldDefaults::colors())] colors: TextFieldColors,
    #[default(crate::TextFieldDefaults::shape())] shape: Shape,
    #[default(crate::TextFieldDefaults::FOCUSED_INDICATOR_THICKNESS)]
    focused_indicator_line_thickness: Dp,
    #[default(crate::TextFieldDefaults::UNFOCUSED_INDICATOR_THICKNESS)]
    unfocused_indicator_line_thickness: Dp,
) {
    let focused = interaction_source.collect_is_focused_as_state().get();
    let color = animated_container_color(&colors, enabled, is_error, focused);
    let background_shape = shape.clone();
    Box(
        modifier
            .draw_behind(move |scope| scope.draw_shape(&background_shape, color.get()))
            .indicator_line(
                enabled,
                is_error,
                &interaction_source,
                Some(colors),
                Some(shape),
                focused_indicator_line_thickness,
                unfocused_indicator_line_thickness,
            ),
        Alignment::TOP_START,
        false,
        compose_runtime::callback(|_| {}),
    );
}

/// An outlined field's container, as `OutlinedTextFieldDefaults.Container`: its border and color
/// animated.
#[composable]
#[allow(non_snake_case)]
pub fn OutlinedTextFieldContainer(
    enabled: bool,
    is_error: bool,
    interaction_source: MutableInteractionSource,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::OutlinedTextFieldDefaults::colors())] colors: TextFieldColors,
    #[default(crate::OutlinedTextFieldDefaults::shape())] shape: Shape,
    #[default(crate::OutlinedTextFieldDefaults::FOCUSED_BORDER_THICKNESS)]
    focused_border_thickness: Dp,
    #[default(crate::OutlinedTextFieldDefaults::UNFOCUSED_BORDER_THICKNESS)]
    unfocused_border_thickness: Dp,
) {
    let focused = interaction_source.collect_is_focused_as_state().get();
    let motion = MaterialTheme::motion_scheme();
    let target = colors.indicator_color(enabled, is_error, focused);
    let thickness =
        if focused && enabled { focused_border_thickness } else { unfocused_border_thickness };
    // As `animateBorderStrokeAsState`: disabled, it snaps.
    let (color, thickness) = if enabled {
        (
            compose_animation::animate_color_as_state(
                target,
                motion.fast_effects_spec(),
                "IndicatorColor",
                None,
            )
            .get(),
            compose_animation::animate_dp_as_state(
                thickness,
                motion.fast_spatial_spec(),
                "IndicatorThickness",
                None,
            )
            .get(),
        )
    } else {
        (target, thickness)
    };
    let container_color = animated_container_color(&colors, enabled, is_error, focused);
    let background_shape = shape.clone();
    Box(
        modifier
            .border_with(thickness, color, shape)
            .draw_behind(move |scope| scope.draw_shape(&background_shape, container_color.get())),
        Alignment::TOP_START,
        false,
        compose_runtime::callback(|_| {}),
    );
}

/// As `TextFieldDefaults.Container`'s: fast effects.
fn animated_container_color(
    colors: &TextFieldColors,
    enabled: bool,
    is_error: bool,
    focused: bool,
) -> compose_runtime::State<Color> {
    compose_animation::animate_color_as_state(
        pick(
            enabled,
            is_error,
            focused,
            [
                colors.disabled_container_color,
                colors.error_container_color,
                colors.focused_container_color,
                colors.container_color,
            ],
        ),
        MaterialTheme::motion_scheme().fast_effects_spec(),
        "ContainerColor",
        None,
    )
}

/// Modifiers for a custom text field.
pub trait TextFieldModifiers {
    /// A filled field's bottom line, as `Modifier.indicatorLine`: its color and width follow the
    /// focus `interaction_source` tells, animated, and it's clipped to `shape`. `colors` by
    /// default are `TextFieldDefaults::colors()`, `shape` is `TextFieldDefaults::shape()`.
    #[allow(clippy::too_many_arguments)]
    fn indicator_line(
        self,
        enabled: bool,
        is_error: bool,
        interaction_source: &MutableInteractionSource,
        colors: Option<TextFieldColors>,
        shape: Option<Shape>,
        focused_indicator_line_thickness: Dp,
        unfocused_indicator_line_thickness: Dp,
    ) -> Modifier;
}

impl TextFieldModifiers for Modifier {
    fn indicator_line(
        self,
        enabled: bool,
        is_error: bool,
        interaction_source: &MutableInteractionSource,
        colors: Option<TextFieldColors>,
        shape: Option<Shape>,
        focused_indicator_line_thickness: Dp,
        unfocused_indicator_line_thickness: Dp,
    ) -> Modifier {
        self.then_custom(IndicatorLineElement(IndicatorParams {
            enabled,
            is_error,
            source: interaction_source.clone(),
            colors,
            shape,
            focused_width: focused_indicator_line_thickness,
            unfocused_width: unfocused_indicator_line_thickness,
        }))
    }
}

#[derive(Clone, PartialEq)]
struct IndicatorParams {
    enabled: bool,
    is_error: bool,
    source: MutableInteractionSource,
    colors: Option<TextFieldColors>,
    shape: Option<Shape>,
    focused_width: Dp,
    unfocused_width: Dp,
}

struct IndicatorLineElement(IndicatorParams);

/// What the line's animations need, for its node and its collection of the source.
struct IndicatorState {
    params: RefCell<IndicatorParams>,
    focused: Cell<bool>,
    /// `colors` or the theme's, and the theme's motion: as the node last read them.
    colors: Cell<Option<TextFieldColors>>,
    motion: RefCell<Option<Rc<crate::motion::MotionScheme>>>,
    scope: Cell<Option<compose_runtime::CoroutineScope>>,
    color: RefCell<Option<Animatable<Color>>>,
    width: Animatable<Dp>,
}

impl IndicatorState {
    /// As `invalidateIndicator`: to the targets, at once when disabled.
    fn invalidate(self: &Rc<Self>) {
        let (Some(scope), Some(colors), Some(motion)) =
            (self.scope.get(), self.colors.get(), self.motion.borrow().clone())
        else {
            return;
        };
        let Some(color) = self.color.borrow().clone() else { return };
        let params = self.params.borrow();
        let focused = self.focused.get();
        let target = colors.indicator_color(params.enabled, params.is_error, focused);
        let enabled = params.enabled;
        let spec =
            if enabled { motion.fast_effects_spec() } else { compose_animation::snap(0).into() };
        scope.launch(async move {
            color.animate_to(target, spec).await;
        });
        let width = self.width.clone();
        let target = if focused && enabled { params.focused_width } else { params.unfocused_width };
        let spec =
            if enabled { motion.fast_spatial_spec() } else { compose_animation::snap(0).into() };
        scope.launch(async move {
            width.animate_to(target, spec).await;
        });
    }

    fn read_theme(&self, context: &mut NodeContext<'_>) {
        let colors = self.params.borrow().colors.unwrap_or_else(|| {
            let scheme = context.current_value_of(&crate::theme::LOCAL_COLOR_SCHEME);
            let selection =
                context.current_value_of(&compose_foundation::LOCAL_TEXT_SELECTION_COLORS);
            TextFieldDefaults::colors_of(&scheme, selection)
        });
        self.colors.set(Some(colors));
        self.motion.replace(Some(context.current_value_of(&crate::motion::LOCAL_MOTION_SCHEME)));
    }
}

struct IndicatorLineNode {
    state: Rc<IndicatorState>,
    listener: Option<Rc<compose_ui::Listener>>,
    source_listened: Option<MutableInteractionSource>,
    shapes: Option<Rc<crate::theme::Shapes>>,
    path: IndicatorPath,
    delegates: compose_ui::Delegates,
}

impl IndicatorLineNode {
    /// As `trackFocusState`: not focused until told, without animating to that.
    fn listen(&mut self) {
        let state = self.state.clone();
        let source = state.params.borrow().source.clone();
        state.focused.set(false);
        let mut ongoing = Vec::new();
        let listener_state = state.clone();
        self.listener = Some(source.listen(move |interaction| {
            match interaction {
                compose_ui::Interaction::Focus(compose_ui::FocusInteraction::Focus(focus)) => {
                    ongoing.push(*focus);
                }
                compose_ui::Interaction::Focus(compose_ui::FocusInteraction::Unfocus(focus)) => {
                    ongoing.retain(|it| it != focus);
                }
                _ => return,
            }
            let now = !ongoing.is_empty();
            if listener_state.focused.replace(now) != now {
                listener_state.invalidate();
            }
        }));
        self.source_listened = Some(source);
    }
}

impl ModifierNodeElement for IndicatorLineElement {
    type Node = IndicatorLineNode;
    fn create(&self) -> IndicatorLineNode {
        IndicatorLineNode {
            state: Rc::new(IndicatorState {
                params: RefCell::new(self.0.clone()),
                focused: Cell::new(false),
                colors: Cell::new(None),
                motion: RefCell::new(None),
                scope: Cell::new(None),
                color: RefCell::new(None),
                width: Animatable::new(self.0.unfocused_width),
            }),
            listener: None,
            source_listened: None,
            shapes: None,
            path: IndicatorPath::default(),
            delegates: compose_ui::Delegates::new(),
        }
    }
    fn update(&self, node: &mut IndicatorLineNode) {
        node.state.params.replace(self.0.clone());
        // A context to launch and read the theme with: `on_update_with_context`.
        node.delegates.ask_for_context();
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    fn name(&self) -> &'static str {
        "indicatorLine"
    }
}

impl ModifierNode for IndicatorLineNode {
    fn delegates(&mut self) -> Option<&mut compose_ui::Delegates> {
        Some(&mut self.delegates)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn should_auto_invalidate(&self) -> bool {
        false
    }
    fn is_composition_local_consumer(&self) -> bool {
        true
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.state.scope.set(Some(context.coroutine_scope()));
        self.state.read_theme(context);
        self.shapes = Some(context.current_value_of(&crate::theme::LOCAL_SHAPES));
        self.listen();
        let mut color = self.state.color.borrow_mut();
        if color.is_none() {
            let params = self.state.params.borrow();
            let colors = self.state.colors.get().expect("just read");
            *color = Some(Animatable::new(colors.indicator_color(
                params.enabled,
                params.is_error,
                false,
            )));
        }
    }
    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        self.listener = None;
        self.source_listened = None;
        self.state.scope.set(None);
    }
    fn on_update_with_context(&mut self, context: &mut NodeContext<'_>) {
        self.state.read_theme(context);
        self.shapes = Some(context.current_value_of(&crate::theme::LOCAL_SHAPES));
        if self.source_listened.as_ref() != Some(&self.state.params.borrow().source) {
            self.listen();
        }
        self.state.invalidate();
        context.invalidate_draw();
    }
    fn on_composition_locals_changed(&mut self, context: &mut NodeContext<'_>) {
        self.state.read_theme(context);
        self.shapes = Some(context.current_value_of(&crate::theme::LOCAL_SHAPES));
        self.state.invalidate();
        context.invalidate_draw();
    }
}

impl DrawModifierNode for IndicatorLineNode {
    fn draw_after(&mut self, scope: &mut compose_ui::DrawScope<'_>) {
        let Some(color) = self.state.color.borrow().as_ref().map(Animatable::value) else {
            return;
        };
        let shape = self
            .state
            .params
            .borrow()
            .shape
            .clone()
            .or_else(|| self.shapes.as_ref().map(|shapes| TextFieldDefaults::shape_of(shapes)));
        let Some(shape) = shape else { return };
        draw_indicator(scope, &self.path, &shape, self.state.width.value(), color);
    }
}

/// The line's path as last drawn, as `IndicatorLineNode`'s `CacheDrawModifierNode`: a draw that
/// changes nothing of it (a keystroke's) combines nothing.
#[derive(Default)]
struct IndicatorPath(RefCell<Option<(IndicatorPathKey, Path)>>);

type IndicatorPathKey = (compose_ui::Size, f32, compose_ui::LayoutDirection, Shape);

/// The line at the bottom, within `shape`, as `IndicatorLineNode`'s drawing.
fn draw_indicator(
    scope: &mut compose_ui::DrawScope<'_>,
    cache: &IndicatorPath,
    shape: &Shape,
    width: Dp,
    color: Color,
) {
    let density = scope.density();
    let size = scope.size();
    let stroke = density.to_px(width);
    let direction = scope.layout_direction();
    let mut cached = cache.0.borrow_mut();
    let fresh = matches!(&*cached, Some(((at, drawn, along, of), _))
        if *at == size && *drawn == stroke && *along == direction && of == shape);
    if !fresh {
        let outline = Path::new();
        crate::tooltip::add_outline(&outline, shape.outline_in(size, &density, direction));
        let line = Path::new();
        line.add_rect(compose_ui::Rect::new(0.0, size.height - stroke, size.width, size.height));
        let clipped = Path::combine(compose_ui::PathOperation::Intersect, &line, &outline);
        *cached = Some(((size, stroke, direction, shape.clone()), clipped));
    }
    if let Some((_, path)) = &*cached {
        scope.draw_path(path, Paint::fill(color));
    }
}

/// What a text field's animations follow, as `InputPhase`.
#[derive(Clone, Copy, PartialEq)]
enum InputPhase {
    Focused,
    UnfocusedEmpty,
    UnfocusedNotEmpty,
}

/// A filled text field, as Compose's `TextField`: a container with an indicator line (2 dp
/// focused), a label resting in the field that floats above the text when focused or filled,
/// a placeholder while focused and empty, optional leading and trailing icons (in 48 dp
/// boxes), a prefix and a suffix around the text (shown with the floated label), and
/// supporting text below; `is_error` shows it all in the error color. A multi-line field is
/// between `min_lines` and `max_lines` lines high.
#[composable]
#[allow(non_snake_case)]
pub fn TextField(
    value: String,
    on_value_change: impl Fn(String) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(false)] read_only: bool,
    #[default(crate::LOCAL_TEXT_STYLE.current())] text_style: TextStyle,
    #[default(None)] label: Option<impl Fn() + 'static>,
    #[default(crate::TextFieldLabelPosition::inside())] label_position: TextFieldLabelPosition,
    #[default(None)] placeholder: Option<impl Fn() + 'static>,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(None)] prefix: Option<impl Fn() + 'static>,
    #[default(None)] suffix: Option<impl Fn() + 'static>,
    #[default(None)] supporting_text: Option<impl Fn() + 'static>,
    #[default(false)] is_error: bool,
    #[default(crate::VisualTransformation::NONE)] visual_transformation: impl Into<VisualTransformation>,
    #[default(crate::KeyboardOptions::DEFAULT)] keyboard_options: KeyboardOptions,
    #[default(crate::KeyboardActions::new())] keyboard_actions: KeyboardActions,
    #[default(false)] single_line: bool,
    #[default(u32::MAX)] max_lines: u32,
    #[default(1)] min_lines: u32,
    #[default(crate::TextFieldDefaults::colors())] colors: TextFieldColors,
    #[default(crate::TextFieldDefaults::shape())] shape: Shape,
    #[default(None)] content_padding: Option<PaddingValues>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    decorated_field(
        Variant::Filled,
        value,
        compose_runtime::callback(on_value_change),
        modifier,
        enabled,
        read_only,
        text_style,
        label_position,
        Slots {
            label: label.map(compose_runtime::callback),
            placeholder: placeholder.map(compose_runtime::callback),
            leading_icon: leading_icon.map(compose_runtime::callback),
            trailing_icon: trailing_icon.map(compose_runtime::callback),
            prefix: prefix.map(compose_runtime::callback),
            suffix: suffix.map(compose_runtime::callback),
            supporting_text: supporting_text.map(compose_runtime::callback),
        },
        is_error,
        visual_transformation,
        keyboard_options,
        keyboard_actions,
        single_line,
        (max_lines, min_lines),
        colors,
        shape,
        content_padding,
        interaction_source,
    );
}

/// An outlined text field, as Compose's `OutlinedTextField`: an outline (2 dp focused) whose
/// top edge the floated label sits on, cut around it, 8 dp above the field for it; with the
/// filled field's icons, prefix and suffix, supporting text, error state and line bounds.
#[composable]
#[allow(non_snake_case)]
pub fn OutlinedTextField(
    value: String,
    on_value_change: impl Fn(String) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(false)] read_only: bool,
    #[default(crate::LOCAL_TEXT_STYLE.current())] text_style: TextStyle,
    #[default(None)] label: Option<impl Fn() + 'static>,
    #[default(crate::TextFieldLabelPosition::cutout())] label_position: TextFieldLabelPosition,
    #[default(None)] placeholder: Option<impl Fn() + 'static>,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(None)] prefix: Option<impl Fn() + 'static>,
    #[default(None)] suffix: Option<impl Fn() + 'static>,
    #[default(None)] supporting_text: Option<impl Fn() + 'static>,
    #[default(false)] is_error: bool,
    #[default(crate::VisualTransformation::NONE)] visual_transformation: impl Into<VisualTransformation>,
    #[default(crate::KeyboardOptions::DEFAULT)] keyboard_options: KeyboardOptions,
    #[default(crate::KeyboardActions::new())] keyboard_actions: KeyboardActions,
    #[default(false)] single_line: bool,
    #[default(u32::MAX)] max_lines: u32,
    #[default(1)] min_lines: u32,
    #[default(crate::OutlinedTextFieldDefaults::colors())] colors: TextFieldColors,
    #[default(crate::OutlinedTextFieldDefaults::shape())] shape: Shape,
    #[default(None)] content_padding: Option<PaddingValues>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    decorated_field(
        Variant::Outlined,
        value,
        compose_runtime::callback(on_value_change),
        modifier,
        enabled,
        read_only,
        text_style,
        label_position,
        Slots {
            label: label.map(compose_runtime::callback),
            placeholder: placeholder.map(compose_runtime::callback),
            leading_icon: leading_icon.map(compose_runtime::callback),
            trailing_icon: trailing_icon.map(compose_runtime::callback),
            prefix: prefix.map(compose_runtime::callback),
            suffix: suffix.map(compose_runtime::callback),
            supporting_text: supporting_text.map(compose_runtime::callback),
        },
        is_error,
        visual_transformation,
        keyboard_options,
        keyboard_actions,
        single_line,
        (max_lines, min_lines),
        colors,
        shape,
        content_padding,
        interaction_source,
    );
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Variant {
    Filled,
    Outlined,
}

/// A decorated field's optional content.
#[derive(Clone)]
struct Slots {
    label: Option<Callback>,
    placeholder: Option<Callback>,
    leading_icon: Option<Callback>,
    trailing_icon: Option<Callback>,
    prefix: Option<Callback>,
    suffix: Option<Callback>,
    supporting_text: Option<Callback>,
}

/// The color for the field's state: disabled, in error, focused, or neither.
fn pick(enabled: bool, error: bool, focused: bool, colors: [Color; 4]) -> Color {
    let [disabled, error_color, focused_color, unfocused] = colors;
    match (enabled, error, focused) {
        (false, _, _) => disabled,
        (true, true, _) => error_color,
        (true, false, true) => focused_color,
        (true, false, false) => unfocused,
    }
}

/// What `decoration_box` decorates.
struct DecorationParts {
    variant: Variant,
    /// The text as it shows is empty, as Compose's `visualText`.
    empty: bool,
    is_focused: bool,
    /// What tells `is_focused`, on the container.
    focus_modifier: Modifier,
    /// At least `MIN_WIDTH` by `MIN_HEIGHT`: a field's, not a bare decoration box's.
    min_size: bool,
    enabled: bool,
    is_error: bool,
    single_line: bool,
    label_position: TextFieldLabelPosition,
    slots: Slots,
    colors: TextFieldColors,
    content_padding: PaddingValues,
    shape: Shape,
    /// `None`, the default container, drawn by the layout's node; else a slot behind the rest.
    container: Option<Callback>,
}

/// The decoration of a field around its `inner_text_field`, as Compose's `CommonDecorationBox`.
fn decoration_box(parts: DecorationParts, inner_text_field: Callback) {
    let DecorationParts {
        variant,
        empty,
        is_focused,
        focus_modifier,
        min_size,
        enabled,
        is_error,
        single_line,
        label_position,
        slots,
        colors,
        content_padding,
        shape,
        container: container_slot,
    } = parts;
    let has_label = slots.label.is_some();
    let shows_expanded = has_label && label_position.shows_expanded();
    // A label above is apart from the rest: to the layout, there's none.
    let label_in_field = has_label && !label_position.is_above();
    let is_above = has_label && label_position.is_above();
    let phase = if is_focused {
        InputPhase::Focused
    } else if empty {
        InputPhase::UnfocusedEmpty
    } else {
        InputPhase::UnfocusedNotEmpty
    };
    let motion = MaterialTheme::motion_scheme();
    let transition = compose_animation::update_transition(phase, "TextFieldInputState");
    // Each part's animation only if it's there, as Compose's, each read where it's used: a frame of
    // them composes neither this nor the rest of the field.
    // 0 resting in the field, 1 floated, as `labelProgress`.
    let progress = has_label.then(|| {
        let spec = motion.fast_spatial_spec();
        transition.animate_float_with(
            move |_| spec.clone(),
            "LabelProgress",
            move |phase| {
                if *phase == InputPhase::UnfocusedEmpty && shows_expanded { 0.0 } else { 1.0 }
            },
        )
    });
    // A placeholder fades in slowly as the field empties or is focused, as `placeholderOpacity`.
    let placeholder_alpha = slots.placeholder.is_some().then(|| {
        let (fast, slow) = (motion.fast_effects_spec(), motion.slow_effects_spec());
        transition.animate_float_with(
            move |segment| match (&segment.initial_state, &segment.target_state) {
                (InputPhase::Focused, InputPhase::UnfocusedEmpty) => fast.clone(),
                (InputPhase::UnfocusedEmpty, InputPhase::Focused)
                | (InputPhase::UnfocusedNotEmpty, InputPhase::UnfocusedEmpty) => slow.clone(),
                _ => fast.clone(),
            },
            "PlaceholderOpacity",
            move |phase| match phase {
                InputPhase::Focused => 1.0,
                InputPhase::UnfocusedEmpty => {
                    if shows_expanded {
                        0.0
                    } else {
                        1.0
                    }
                }
                InputPhase::UnfocusedNotEmpty => 0.0,
            },
        )
    });
    // The prefix and suffix, shown as the label floats (or always, without one), as `affixOpacity`.
    let affix_alpha = (slots.prefix.is_some() || slots.suffix.is_some()).then(|| {
        let spec = motion.fast_effects_spec();
        transition.animate_float_with(
            move |_| spec.clone(),
            "PrefixSuffixOpacity",
            move |phase| {
                if *phase == InputPhase::UnfocusedEmpty && shows_expanded { 0.0 } else { 1.0 }
            },
        )
    });
    // Transparent components interfere with screen readers: at alpha 0 they're not there at all, as
    // Compose's `showPlaceholder` and `showAffix`, crossing 0 composing this again.
    let shown = |alpha: Option<compose_runtime::State<f32>>| {
        compose_runtime::remember_keyed(alpha.is_some(), move || {
            Rc::new(compose_runtime::derived_state_of(move || {
                alpha.is_some_and(|alpha| alpha.get() > 0.0)
            }))
        })
        .get()
    };
    let show_placeholder_alpha = shown(placeholder_alpha);
    let show_affix = shown(affix_alpha);
    let indicator = pick(
        enabled,
        is_error,
        is_focused,
        [
            colors.disabled_indicator_color,
            colors.error_indicator_color,
            colors.focused_indicator_color,
            colors.unfocused_indicator_color,
        ],
    );
    // As `animateBorderStrokeAsState`: disabled, it snaps.
    let indicator_state = enabled.then(|| {
        compose_animation::animate_color_as_state(
            indicator,
            motion.fast_effects_spec(),
            "IndicatorColor",
            None,
        )
    });
    let (focused_thickness, unfocused_thickness) = match variant {
        Variant::Filled => (
            TextFieldDefaults::FOCUSED_INDICATOR_THICKNESS,
            TextFieldDefaults::UNFOCUSED_INDICATOR_THICKNESS,
        ),
        Variant::Outlined => (
            OutlinedTextFieldDefaults::FOCUSED_BORDER_THICKNESS,
            OutlinedTextFieldDefaults::UNFOCUSED_BORDER_THICKNESS,
        ),
    };
    let thickness_state = enabled.then(|| {
        compose_animation::animate_dp_as_state(
            if is_focused { focused_thickness } else { unfocused_thickness },
            motion.fast_spatial_spec(),
            "IndicatorThickness",
            None,
        )
    });
    let indicator_color = move || indicator_state.map_or(indicator, |state| state.get());
    let indicator_thickness =
        move || thickness_state.map_or(unfocused_thickness, |state| state.get());
    let text_color = if enabled { colors.text_color } else { colors.disabled_text_color };
    let typography = MaterialTheme::typography();
    // Body large around the text, whatever the text's own, as `CommonDecorationBox`.
    let text_style = typography.body_large.clone();
    let style = TextStyle { color: text_style.color.or(text_color), ..text_style.clone() };
    let label_color = pick(
        enabled,
        is_error,
        is_focused,
        [
            colors.disabled_label_color,
            colors.error_label_color,
            colors.focused_label_color,
            colors.label_color,
        ],
    );
    let leading_color = pick(
        enabled,
        is_error,
        is_focused,
        [
            colors.disabled_leading_icon_color,
            colors.error_leading_icon_color,
            colors.leading_icon_color,
            colors.leading_icon_color,
        ],
    );
    let trailing_color = pick(
        enabled,
        is_error,
        is_focused,
        [
            colors.disabled_trailing_icon_color,
            colors.error_trailing_icon_color,
            colors.trailing_icon_color,
            colors.trailing_icon_color,
        ],
    );
    let supporting_color = pick(
        enabled,
        is_error,
        is_focused,
        [
            colors.disabled_supporting_text_color,
            colors.error_supporting_text_color,
            colors.supporting_text_color,
            colors.supporting_text_color,
        ],
    );
    // The prefix and suffix in it too.
    let prefix_style = TextStyle {
        color: if enabled { colors.prefix_color } else { colors.disabled_prefix_color },
        ..text_style.clone()
    };
    let suffix_style = TextStyle {
        color: if enabled { colors.suffix_color } else { colors.disabled_suffix_color },
        ..text_style.clone()
    };
    // As `TextFieldDefaults.Container`: the color read as it draws (`textFieldBackground`).
    let container_color = compose_animation::animate_color_as_state(
        pick(
            enabled,
            is_error,
            is_focused,
            [
                colors.disabled_container_color,
                colors.error_container_color,
                colors.focused_container_color,
                colors.container_color,
            ],
        ),
        motion.fast_effects_spec(),
        "ContainerColor",
        None,
    );
    let placeholder_color =
        if enabled { colors.placeholder_color } else { colors.disabled_placeholder_color };
    // Shown while the field is empty and it's not transparent, as Compose's.
    let show_placeholder = empty && slots.placeholder.is_some() && show_placeholder_alpha;
    let (has_prefix, has_suffix) =
        (slots.prefix.is_some() && show_affix, slots.suffix.is_some() && show_affix);
    let floated_label_height = typography.body_small.line_height;
    let supporting_style = typography.body_small.clone();
    // Remembered: the layout (remembered too) writes it, the outline reads it. Not state: the
    // outline draws in the node the layout measures, so measuring it draws it again (a reader
    // in another repaint boundary would draw a stale cut, docs/06).
    let label_width = remember(|| Rc::new(std::cell::Cell::new(0.0f32)));
    // The outline's path, reset each time it's drawn, as Compose's drawing caches keep one
    // (`CheckDrawingCache`): an animated cut redefines it in place on Android (D532).
    let outline = remember(Path::new);
    let line_path = remember(|| Rc::new(IndicatorPath::default()));
    let mut container = focus_modifier;
    if min_size {
        container = container.size_in(SizeSpec::bounds(
            TextFieldDefaults::MIN_WIDTH,
            TextFieldDefaults::MIN_HEIGHT,
            Dp::UNSPECIFIED,
            Dp::UNSPECIFIED,
        ));
    }
    let cutout = label_in_field && label_position.is_cutout();
    let notch_alignment = label_position.minimized_alignment();
    // Where the cut goes, as `outlineCutout`: at the minimized label, as wide as it is yet,
    // left to right (a right-to-left outline mirrors it).
    let notch = {
        let label_width = label_width.clone();
        move |width: f32, density: compose_ui::Density| {
            if !cutout {
                return (0.0, 0.0);
            }
            let shown = label_width.get() * progress.map_or(0.0, |progress| progress.get());
            let padding = density.to_px(content_padding.start);
            let space = (width - padding - density.to_px(content_padding.end)).round() as i32;
            let x = notch_alignment.align(
                shown.round() as i32,
                space,
                compose_ui::LayoutDirection::Ltr,
            );
            (padding + x as f32, shown)
        }
    };
    let custom_container = container_slot.is_some();
    let slot_notch = notch.clone();
    container = match variant {
        _ if custom_container => container,
        Variant::Filled => {
            let shape = shape.clone();
            let line_path = line_path.clone();
            container.draw_behind(move |scope| {
                let (start, shown) = notch(scope.size().width, scope.density());
                if shown <= 0.0 {
                    scope.draw_shape(&shape, container_color.get());
                    draw_indicator(
                        scope,
                        &line_path,
                        &shape,
                        indicator_thickness(),
                        indicator_color(),
                    );
                    return;
                }
                // The container's cut as the outline's is, under the label.
                let half = scope.density().sp_to_px(floated_label_height) / 2.0;
                draw_cut_container(scope, &shape, container_color.get(), start, shown, half);
                draw_indicator(scope, &line_path, &shape, indicator_thickness(), indicator_color());
            })
        }
        Variant::Outlined => {
            let shape = shape.clone();
            let outline = outline.clone();
            container.draw_behind(move |scope| {
                scope.draw_shape(&shape, container_color.get());
                let (start, shown) = notch(scope.size().width, scope.density());
                draw_notched_outline(
                    scope,
                    &outline,
                    &shape,
                    indicator_thickness(),
                    indicator_color(),
                    start,
                    shown,
                )
            })
        }
    };
    let layout = DecorationLayout {
        variant,
        has_leading: slots.leading_icon.is_some(),
        has_trailing: slots.trailing_icon.is_some(),
        has_container: custom_container,
        content_padding,
        has_label: label_in_field,
        position: label_position,
        has_placeholder: show_placeholder,
        has_prefix,
        has_suffix,
        single_line,
        progress,
        floated_label_height,
        label_width: label_width.clone(),
    };
    let policy = compose_runtime::remember_keyed(
        LayoutKey(layout.key(), label_position, content_padding),
        || Rc::new(layout) as Rc<dyn MeasurePolicy>,
    );
    let slots = slots.clone();
    let supporting_text = slots.supporting_text.clone();
    let above_alignment = is_above.then(|| label_position.minimized_alignment());
    let around =
        compose_runtime::remember_keyed(above_alignment.map(|it| it.bias.to_bits()), || {
            Rc::new(FieldColumn { above_alignment }) as Rc<dyn MeasurePolicy>
        });
    let above_label = slots.label.clone().filter(|_| is_above);
    let style = style.clone();
    let (prefix_style, suffix_style) = (prefix_style.clone(), suffix_style.clone());
    let content = compose_runtime::callback(move || {
        // Behind everything, as big as it all, as Compose's `ContainerId`.
        if let Some(container) = container_slot.clone() {
            let notch = slot_notch.clone();
            // The label's room out of it, as `outlineCutout`.
            let cut = Modifier.draw_with_content(move |scope| {
                let (start, shown) = notch(scope.size().width, scope.density());
                if shown <= 0.0 {
                    scope.draw_content();
                    return;
                }
                let half = scope.density().sp_to_px(floated_label_height) / 2.0;
                let path = cut_container_path(scope, start, shown, half);
                scope.save();
                scope.clip_outline(compose_ui::Outline::Generic(
                    path.data(),
                    compose_ui::Offset::ZERO,
                ));
                scope.draw_content();
                scope.restore();
            });
            Box(cut, Alignment::TOP_START, true, compose_runtime::callback(move |_| container()));
        }
        // The text first: what focuses the field finds the first focus target below it, and Tab
        // goes from the text to its icons, as Compose's, whose field's target is above them
        // (D1298).
        inner_text_field();
        // Each icon in a 48 dp box, centered, as Compose's `IconDefaultSizeModifier`.
        for (icon, color) in
            [(&slots.leading_icon, leading_color), (&slots.trailing_icon, trailing_color)]
        {
            if let Some(icon) = icon.clone() {
                CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
                    let icon = icon.clone();
                    Box(
                        Modifier.size_in(SizeSpec::bounds(
                            48.dp(),
                            48.dp(),
                            Dp::UNSPECIFIED,
                            Dp::UNSPECIFIED,
                        )),
                        Alignment::CENTER,
                        false,
                        compose_runtime::callback(move |_| icon()),
                    );
                });
            }
        }
        if let (Some(label), Some(progress)) =
            (slots.label.clone().filter(|_| label_in_field), progress)
        {
            FieldLabel(progress, label_color, is_focused, label);
        }
        if show_placeholder && let Some(placeholder) = slots.placeholder.clone() {
            let style = style.clone();
            let placeholder_modifier = Modifier.graphics_layer(move |layer| {
                layer.alpha = placeholder_alpha.map_or(0.0, |alpha| alpha.get());
            });
            Box!(modifier = placeholder_modifier, {
                CompositionLocalProvider(
                    (
                        LOCAL_CONTENT_COLOR.provides(placeholder_color),
                        LOCAL_TEXT_STYLE.provides(style.clone()),
                    ),
                    &*placeholder,
                );
            });
        }
        // As Compose's `PrefixSuffixTextPadding`: 2 dp between them and the text.
        for (content, shows, style, padding) in [
            (&slots.prefix, has_prefix, prefix_style.clone(), (0.dp(), 2.dp())),
            (&slots.suffix, has_suffix, suffix_style.clone(), (2.dp(), 0.dp())),
        ] {
            if let Some(content) = content.clone().filter(|_| shows) {
                CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(style), || {
                    let content = content.clone();
                    Box(
                        Modifier
                            .graphics_layer(move |layer| {
                                layer.alpha = affix_alpha.map_or(0.0, |alpha| alpha.get());
                            })
                            .padding((padding.0, 0.dp(), padding.1, 0.dp())),
                        Alignment::TOP_START,
                        false,
                        compose_runtime::callback(move |_| content()),
                    );
                });
            }
        }
    });
    if above_label.is_none() && supporting_text.is_none() {
        compose_ui::Layout(container, policy, content);
        return;
    }
    let supporting_style = supporting_style.clone();
    compose_ui::Layout(
        Modifier,
        around,
        compose_runtime::callback(move || {
            // The label above, 4 dp from the field and its sides, as
            // `AboveLabelHorizontalPadding` and `AboveLabelBottomPadding`.
            if let (Some(label), Some(progress)) = (above_label.clone(), progress) {
                Box!(modifier = Modifier.padding((4.dp(), 0.dp(), 4.dp(), 4.dp())), {
                    FieldLabel(progress, label_color, is_focused, label.clone());
                });
            }
            compose_ui::Layout(container.clone(), policy.clone(), content.clone());
            // The supporting text below the container, 16 dp in, 4 dp under it.
            if let Some(supporting_text) = supporting_text.clone() {
                let style = supporting_style.clone();
                CompositionLocalProvider(
                    (
                        LOCAL_CONTENT_COLOR.provides(supporting_color),
                        LOCAL_TEXT_STYLE.provides(style),
                    ),
                    move || {
                        let supporting_text = supporting_text.clone();
                        Box(
                            Modifier.padding((16.dp(), 4.dp(), 16.dp(), 0.dp())),
                            Alignment::TOP_START,
                            false,
                            compose_runtime::callback(move |_| supporting_text()),
                        );
                    },
                );
            }
        }),
    );
}

#[allow(clippy::too_many_arguments)]
fn decorated_field(
    variant: Variant,
    value: String,
    on_value_change: Callback<dyn Fn(String)>,
    modifier: Modifier,
    enabled: bool,
    read_only: bool,
    text_style: TextStyle,
    label_position: TextFieldLabelPosition,
    slots: Slots,
    is_error: bool,
    visual_transformation: VisualTransformation,
    keyboard_options: KeyboardOptions,
    keyboard_actions: KeyboardActions,
    single_line: bool,
    (max_lines, min_lines): (u32, u32),
    colors: TextFieldColors,
    shape: Shape,
    content_padding: Option<PaddingValues>,
    interaction_source: Option<MutableInteractionSource>,
) {
    let focused = remember(|| mutable_state_of(false));
    let is_focused = focused.get();
    // Empty as it shows, as Compose's `visualText`.
    let empty = visual_transformation
        .filter(&value)
        .map_or(value.is_empty(), |transformed| transformed.text.is_empty());
    let has_label = slots.label.is_some();
    let cursor_color = if is_error { colors.error_cursor_color } else { colors.cursor_color };
    let text_color = if enabled { colors.text_color } else { colors.disabled_text_color };
    let style = TextStyle { color: text_style.color.or(text_color), ..text_style.clone() };
    let content_padding =
        content_padding.unwrap_or(default_content_padding(has_label, label_position));
    let decoration = move |inner_text_field: Callback| {
        decoration_box(
            DecorationParts {
                variant,
                empty,
                is_focused,
                focus_modifier: Modifier.on_focus_changed(move |now: compose_ui::FocusState| {
                    focused.set(now.is_focused())
                }),
                min_size: true,
                enabled,
                is_error,
                single_line,
                label_position,
                slots: slots.clone(),
                colors,
                content_padding,
                shape: shape.clone(),
                container: None,
            },
            inner_text_field,
        );
    };
    // Room above for a label on the edge, as `topPaddingForLabelCutout`.
    let modifier = if has_label && label_position.is_cutout() {
        modifier.padding((0.dp(), 8.dp(), 0.dp(), 0.dp()))
    } else {
        modifier
    };
    // In error, the field says so, as Compose's `semantics { error(…) }`.
    let modifier = if is_error {
        modifier.semantics(|config| config.error("Invalid input"))
    } else {
        modifier
    };
    // Its selection in its colors' selection colors, as Compose's fields provide
    // `colors.textSelectionColors`.
    compose_runtime::CompositionLocalProvider(
        compose_foundation::LOCAL_TEXT_SELECTION_COLORS.provides(colors.text_selection_colors),
        || {
            BasicTextField(
                value,
                on_value_change,
                modifier,
                enabled,
                read_only,
                style,
                keyboard_options,
                keyboard_actions,
                single_line,
                max_lines,
                min_lines,
                visual_transformation,
                cursor_color,
                interaction_source,
                compose_runtime::callback(decoration),
            );
        },
    );
}

#[derive(Clone, Copy, PartialEq)]
struct LayoutKey((Variant, [bool; 8]), TextFieldLabelPosition, PaddingValues);

impl compose_runtime::ComposeArg for LayoutKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The decoration's layout, as Compose's `TextFieldMeasurePolicy` and
/// `OutlinedTextFieldMeasurePolicy`: the leading and trailing icons (if any), the label (if
/// any), the placeholder (if shown), the prefix and suffix (if any), the text field, then
/// (filled) the indicator line. The prefix is before the text, the suffix at the end. 16 dp at
/// the sides (4 dp beside an icon, as `HorizontalIconPadding` takes 12 of them); a filled field
/// 8 dp above and below with a label (16 dp without), its text below where the floated label
/// goes; an outlined field 16 dp above and below, its floated label centered on the top edge,
/// 16 dp in. The icons are centered vertically. The label rests where a single line of text
/// would be (or at the top padding) and moves to its floated place with `progress`.
struct DecorationLayout {
    variant: Variant,
    has_container: bool,
    content_padding: PaddingValues,
    has_leading: bool,
    has_trailing: bool,
    has_label: bool,
    position: TextFieldLabelPosition,
    has_placeholder: bool,
    has_prefix: bool,
    has_suffix: bool,
    single_line: bool,
    /// The label's: read while measuring, each frame of it laying the field out.
    progress: Option<compose_runtime::State<f32>>,
    floated_label_height: compose_ui::TextUnit,
    /// The label's width, for the outline's cut.
    label_width: Rc<std::cell::Cell<f32>>,
}

/// A decoration's slots, as its children come.
struct DecorationSlots {
    leading: Option<Measurable>,
    trailing: Option<Measurable>,
    label: Option<Measurable>,
    placeholder: Option<Measurable>,
    prefix: Option<Measurable>,
    suffix: Option<Measurable>,
    text: Option<Measurable>,
}

impl DecorationLayout {
    fn slots(&self, measurables: &[Measurable]) -> DecorationSlots {
        let mut children = measurables.iter().copied();
        let mut next = |present: bool| if present { children.next() } else { None };
        next(self.has_container);
        DecorationSlots {
            text: next(true),
            leading: next(self.has_leading),
            trailing: next(self.has_trailing),
            label: next(self.has_label),
            placeholder: next(self.has_placeholder),
            prefix: next(self.has_prefix),
            suffix: next(self.has_suffix),
        }
    }

    /// The paddings around the content, given which icons there are: (start, end, top, bottom),
    /// less `HorizontalIconPadding` beside an icon.
    fn paddings(
        &self,
        density: compose_ui::Density,
        has_leading: bool,
        has_trailing: bool,
    ) -> (i32, i32, i32, i32) {
        let padding = self.content_padding;
        let icon_padding = density.round_to_px(12.dp());
        let beside = |px: i32, icon: bool| if icon { (px - icon_padding).max(0) } else { px };
        (
            beside(density.round_to_px(padding.start), has_leading),
            beside(density.round_to_px(padding.end), has_trailing),
            density.round_to_px(padding.top),
            density.round_to_px(padding.bottom),
        )
    }

    /// The floated label's room above the text, inside: for what's asked before there's a label.
    fn label_room(&self, density: compose_ui::Density) -> i32 {
        if self.has_label && self.position.is_inside() {
            density.sp_to_px(self.floated_label_height).round() as i32
        } else {
            0
        }
    }

    /// As Compose's `intrinsicWidth`: the icons beside the widest of the text and the
    /// placeholder with the affixes, and the label.
    fn intrinsic_width(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[Measurable],
        height: i32,
        max: bool,
    ) -> i32 {
        let slots = self.slots(measurables);
        let Some(text) = slots.text else { return 0 };
        let (start, end, _, _) =
            self.paddings(scope.density(), slots.leading.is_some(), slots.trailing.is_some());
        let mut ask = |slot: Option<Measurable>| {
            slot.map_or(0, |slot| {
                if max {
                    scope.max_intrinsic_width(slot, height)
                } else {
                    scope.min_intrinsic_width(slot, height)
                }
            })
        };
        let (leading, trailing) = (ask(slots.leading), ask(slots.trailing));
        let affixes = ask(slots.prefix) + ask(slots.suffix);
        let middle =
            (ask(Some(text)) + affixes).max(ask(slots.placeholder) + affixes).max(ask(slots.label));
        leading + start + middle + end + trailing
    }

    /// As Compose's `intrinsicHeight`: the content at the width left by the icons and the
    /// affixes, between the paddings and the floated label's room, and the icons.
    fn intrinsic_height(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[Measurable],
        width: i32,
        max: bool,
    ) -> i32 {
        let slots = self.slots(measurables);
        let Some(text) = slots.text else { return 0 };
        let density = scope.density();
        let (start, end, top, bottom) =
            self.paddings(density, slots.leading.is_some(), slots.trailing.is_some());
        let label_room = self.label_room(density);
        let less = |width: i32, by: i32| {
            if width == Constraints::INFINITY { width } else { (width - by).max(0) }
        };
        let ask = |scope: &mut compose_ui::IntrinsicMeasureScope<'_>, slot, width| {
            if max {
                scope.max_intrinsic_height(slot, width)
            } else {
                scope.min_intrinsic_height(slot, width)
            }
        };
        let mut remaining = less(width, start + end);
        let mut icons = 0;
        for icon in [slots.leading, slots.trailing].into_iter().flatten() {
            icons = icons.max(ask(scope, icon, width));
            let icon_width = scope.max_intrinsic_width(icon, Constraints::INFINITY);
            remaining = less(remaining, icon_width);
        }
        let label = slots.label.map_or(0, |label| ask(scope, label, remaining));
        let mut content = 0;
        for affix in [slots.prefix, slots.suffix].into_iter().flatten() {
            content = content.max(ask(scope, affix, remaining));
            let affix_width = scope.max_intrinsic_width(affix, Constraints::INFINITY);
            remaining = less(remaining, affix_width);
        }
        content = content.max(ask(scope, text, remaining));
        if let Some(placeholder) = slots.placeholder {
            content = content.max(ask(scope, placeholder, remaining));
        }
        // The label floats in its room (filled) or on the edge (outlined), as `measure` has
        // it; resting, it's no higher than the text.
        let _ = label;
        (top + label_room + content + bottom).max(icons)
    }

    fn key(&self) -> (Variant, [bool; 8]) {
        (
            self.variant,
            [
                self.has_container,
                self.has_leading,
                self.has_trailing,
                self.has_label,
                self.has_placeholder,
                self.has_prefix,
                self.has_suffix,
                self.single_line,
            ],
        )
    }
}

impl MeasurePolicy for DecorationLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let inside = self.position.is_inside();
        let mut children = measurables.iter().copied();
        let container = if self.has_container { children.next() } else { None };
        let Some(text) = children.next() else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let leading = if self.has_leading { children.next() } else { None };
        let trailing = if self.has_trailing { children.next() } else { None };
        let label = if self.has_label { children.next() } else { None };
        let placeholder = if self.has_placeholder { children.next() } else { None };
        let prefix = if self.has_prefix { children.next() } else { None };
        let suffix = if self.has_suffix { children.next() } else { None };
        let icon_constraints = constraints.loosen();
        let leading = leading.map(|icon| scope.measure(icon, icon_constraints));
        let trailing = trailing.map(|icon| scope.measure(icon, icon_constraints));
        let leading_width = leading.map_or(0, |icon| icon.width);
        let trailing_width = trailing.map_or(0, |icon| icon.width);
        let (start_padding, end_padding, top, bottom) =
            self.paddings(density, leading.is_some(), trailing.is_some());
        let start = leading_width + start_padding;
        let end = trailing_width + end_padding;
        let loose = constraints.loosen().offset(-start - end, -top - bottom);
        let label = label.map(|label| scope.measure(label, loose));
        let label_height = label.map_or(0, |label| label.height);
        self.label_width.set(label.map_or(0, |label| label.width) as f32);
        // Inside, the text sits below the floated label; on the edge, it's below half of it.
        let above_text = if inside { label_height } else { (label_height / 2 - top).max(0) };
        let prefix = prefix.map(|prefix| scope.measure(prefix, loose.offset(0, -above_text)));
        let suffix = suffix.map(|suffix| scope.measure(suffix, loose.offset(0, -above_text)));
        let prefix_width = prefix.map_or(0, |prefix| prefix.width);
        let suffix_width = suffix.map_or(0, |suffix| suffix.width);
        let mut text_constraints = constraints
            .offset(-start - end - prefix_width - suffix_width, -top - bottom - above_text);
        text_constraints.min_height = 0;
        let text = scope.measure(text, text_constraints);
        let placeholder = placeholder.map(|placeholder| {
            scope.measure(placeholder, loose.offset(-prefix_width - suffix_width, -above_text))
        });
        let content_width = (prefix_width
            + text.width.max(placeholder.map_or(0, |placeholder| placeholder.width))
            + suffix_width)
            .max(label.map_or(0, |label| label.width));
        let progress = self.progress.map_or(1.0, |progress| progress.get());
        let lerp =
            |from: i32, to: i32, along: f32| from + ((to - from) as f32 * along).round() as i32;
        let input_height = text
            .height
            .max(placeholder.map_or(0, |placeholder| placeholder.height))
            .max(prefix.map_or(0, |prefix| prefix.height))
            .max(suffix.map_or(0, |suffix| suffix.height))
            .max(lerp(label_height, 0, progress));
        let icons_height =
            leading.map_or(0, |icon| icon.height).max(trailing.map_or(0, |icon| icon.height));
        // As `calculateHeight`: the label's part in it goes with its move, with a steady least
        // inside so the height holds through the animation.
        let middle = if !self.has_label {
            top + input_height + bottom
        } else if inside {
            let eased = LABEL_HEIGHT_EASING.transform(progress);
            let not_overlapped = density.round_to_px(16.dp()).max(lerp(0, label_height, eased));
            top + not_overlapped + input_height + bottom
        } else {
            lerp(top, top.max(label_height / 2), progress) + input_height + bottom
        };
        let width = constraints.constrain_width(start + content_width + end);
        let height = constraints.constrain_height(middle.max(icons_height));
        let container = container.map(|container| {
            scope.measure(container, Constraints::new(width, width, height, height))
        });
        if let Some(container) = &container {
            scope.place_relative(container, 0, 0);
        }
        // Inside with a label, below it; else centered on a single line, or at the top padding,
        // and on the edge never under the label's half, as `calculateVerticalPosition`.
        let y_of = |item_height: i32| {
            if self.has_label && inside {
                return top + label_height;
            }
            let default = if self.single_line { (height - item_height) / 2 } else { top };
            if self.has_label { default.max(label_height / 2) } else { default }
        };
        if let Some(icon) = &leading {
            scope.place_relative(icon, 0, (height - icon.height) / 2);
        }
        if let Some(icon) = &trailing {
            scope.place_relative(icon, width - icon.width, (height - icon.height) / 2);
        }
        if let Some(label) = &label {
            // A single line centered; else at the top padding (inside, with half the floated
            // label's room).
            let resting = if self.single_line {
                (height - label.height) / 2
            } else if inside {
                top + density.round_to_px(8.dp())
            } else {
                top
            };
            let floated = if inside { top } else { -label.height / 2 };
            let y = lerp(resting, floated, progress);
            // Aligned between the icons' and paddings' room (resting); minimized too, inside,
            // while on the edge it's between the paddings alone, over a leading icon.
            let direction = scope.layout_direction();
            let ltr = direction == compose_ui::LayoutDirection::Ltr;
            let left = if ltr { start } else { end };
            let resting_x = self.position.expanded_alignment().align(
                label.width,
                width - start - end,
                direction,
            ) + left;
            let floated_x = if inside {
                self.position.minimized_alignment().align(
                    label.width,
                    width - start - end,
                    direction,
                ) + left
            } else {
                let (outer_start, outer_end) = (
                    density.round_to_px(self.content_padding.start),
                    density.round_to_px(self.content_padding.end),
                );
                self.position.minimized_alignment().align(
                    label.width,
                    width - outer_start - outer_end,
                    direction,
                ) + if ltr { outer_start } else { outer_end }
            };
            let x = lerp(resting_x, floated_x, progress);
            // Not relative: the alignment has the direction.
            scope.place(label, x, y);
        }
        if let Some(placeholder) = &placeholder {
            scope.place_relative(placeholder, start + prefix_width, y_of(placeholder.height));
        }
        if let Some(prefix) = &prefix {
            scope.place_relative(prefix, start, y_of(prefix.height));
        }
        if let Some(suffix) = &suffix {
            scope.place_relative(suffix, width - end - suffix.width, y_of(suffix.height));
        }
        scope.place_relative(&text, start + prefix_width, y_of(text.height));
        scope.layout(width, height)
    }

    // As Compose's `TextFieldMeasurePolicy` and `OutlinedTextFieldMeasurePolicy`: the slots'
    // intrinsics in the layout's arrangement, without measuring (its measurement keeps the
    // label's place for the outline) (D1207).
    fn min_intrinsic_width(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[compose_ui::IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        self.intrinsic_width(scope, measurables, height, false)
    }

    fn max_intrinsic_width(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[compose_ui::IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        self.intrinsic_width(scope, measurables, height, true)
    }

    fn min_intrinsic_height(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[compose_ui::IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        self.intrinsic_height(scope, measurables, width, false)
    }

    fn max_intrinsic_height(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[compose_ui::IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        self.intrinsic_height(scope, measurables, width, true)
    }

    fn debug_name(&self) -> &'static str {
        "TextFieldDecoration"
    }
}

/// The field, then its supporting text below it, as wide as the field (wrapping within it),
/// as Compose's text field layouts measure it.
struct FieldColumn {
    /// With a label above, first of the children, how it's aligned.
    above_alignment: Option<HorizontalAlignment>,
}

impl MeasurePolicy for FieldColumn {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let (label, rest) = match (self.above_alignment, measurables) {
            (Some(_), [label, rest @ ..]) => (Some(*label), rest),
            _ => (None, measurables),
        };
        let [field, supporting @ ..] = rest else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        // Measured after the field, whose width it has: its height kept for it first.
        let reserved =
            label.map_or(0, |label| scope.min_intrinsic_height(label, constraints.min_width));
        let mut field_constraints = constraints;
        field_constraints.min_height = 0;
        if field_constraints.max_height != Constraints::INFINITY {
            field_constraints.max_height = (field_constraints.max_height - reserved).max(0);
        }
        let field = scope.measure(*field, field_constraints);
        let label =
            label.map(|label| scope.measure(label, Constraints::new(0, field.width, 0, reserved)));
        let label_height = label.map_or(0, |label| label.height);
        let supporting = supporting.first().map(|supporting| {
            scope.measure(*supporting, Constraints::new(0, field.width, 0, constraints.max_height))
        });
        if let (Some(label), Some(alignment)) = (&label, self.above_alignment) {
            let direction = scope.layout_direction();
            // Not relative: the alignment has the direction.
            scope.place(label, alignment.align(label.width, field.width, direction), 0);
        }
        scope.place_relative(&field, 0, label_height);
        if let Some(supporting) = &supporting {
            scope.place_relative(supporting, 0, label_height + field.height);
        }
        let height = constraints.constrain_height(
            label_height + field.height + supporting.map_or(0, |supporting| supporting.height),
        );
        scope.layout(field.width, height)
    }

    fn debug_name(&self) -> &'static str {
        "TextFieldSupporting"
    }
}

/// The outline of `shape` around the node, `thickness` wide, its top edge cut for a label
/// `label_width` wide starting `label_start` in (4 dp each side of it), as Compose's
/// `outlineCutout`: built in `path`, reset first.
fn draw_notched_outline(
    scope: &mut compose_ui::DrawScope<'_>,
    path: &Path,
    shape: &Shape,
    thickness: Dp,
    color: Color,
    label_start: f32,
    label_width: f32,
) {
    let density = scope.density();
    let size = scope.size();
    let stroke = density.to_px(thickness);
    let half = stroke / 2.0;
    let radius = match shape.outline(size, &density) {
        compose_ui::Outline::Rounded(_, radii) => radii[0],
        compose_ui::Outline::Rectangle(_) | compose_ui::Outline::Generic(..) => 0.0,
    };
    let radius = (radius - half).max(0.0);
    let (left, top, right, bottom) = (half, half, size.width - half, size.height - half);
    let rtl = scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
    let padding = density.to_px(4.dp());
    let start = label_start - padding;
    let cut = if label_width > 0.0 { label_width + 2.0 * padding } else { 0.0 };
    // The cut in left-to-right x, mirrored in a right-to-left layout.
    let (cut_left, cut_right) =
        if rtl { (size.width - start - cut, size.width - start) } else { (start, start + cut) };
    // From the cut's right end, clockwise round to its left end; corners as quarter circles.
    const K: f32 = 0.552_284_8;
    path.reset();
    path.move_to(if cut > 0.0 { cut_right } else { left + radius }, top);
    path.line_to(right - radius, top);
    path.cubic_to(
        right - radius + radius * K,
        top,
        right,
        top + radius - radius * K,
        right,
        top + radius,
    );
    path.line_to(right, bottom - radius);
    path.cubic_to(
        right,
        bottom - radius + radius * K,
        right - radius + radius * K,
        bottom,
        right - radius,
        bottom,
    );
    path.line_to(left + radius, bottom);
    path.cubic_to(
        left + radius - radius * K,
        bottom,
        left,
        bottom - radius + radius * K,
        left,
        bottom - radius,
    );
    path.line_to(left, top + radius);
    path.cubic_to(
        left,
        top + radius - radius * K,
        left + radius - radius * K,
        top,
        left + radius,
        top,
    );
    if cut > 0.0 {
        path.line_to(cut_left, top);
    } else {
        path.close();
    }
    scope.draw_path(path, Paint::stroke(color, stroke));
}
