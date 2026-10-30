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
    MutableInteractionSource, NodeContext, PaddingValues, Paint, Path, Placeable, Shape, TextStyle,
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
fn FieldLabel(
    modifier: Modifier,
    progress: compose_runtime::State<f32>,
    color: Color,
    focused: bool,
    label: Callback,
) {
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
    Box!(modifier = modifier, {
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
    let visual_text = visual_text_of(&visual_transformation, &value);
    let content_padding =
        content_padding.unwrap_or(default_content_padding(slots.label.is_some(), label_position));
    decoration_box(
        DecorationParts {
            variant,
            visual_text,
            is_focused,
            interaction_source,
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
    /// Where the node reads the theme, for `colors` and `shape` of `None` and the motion, when
    /// they're wanted, as `currentValueOf`.
    locals: Cell<Option<compose_runtime::CompositionLocalAnchor>>,
    scope: Cell<Option<compose_runtime::CoroutineScope>>,
    color: RefCell<Option<Animatable<Color>>>,
    width: Animatable<Dp>,
}

impl IndicatorState {
    fn colors(&self, locals: compose_runtime::CompositionLocalAnchor) -> TextFieldColors {
        self.params.borrow().colors.unwrap_or_else(|| {
            let map = locals.map();
            TextFieldDefaults::colors_of(
                &map.get::<Rc<crate::ColorScheme>>(&crate::theme::LOCAL_COLOR_SCHEME),
                map.get(&compose_foundation::LOCAL_TEXT_SELECTION_COLORS),
            )
        })
    }

    /// As `invalidateIndicator`: to the targets, at once when disabled.
    fn invalidate(self: &Rc<Self>) {
        let (Some(scope), Some(locals)) = (self.scope.get(), self.locals.get()) else { return };
        let Some(color) = self.color.borrow().clone() else { return };
        let colors = self.colors(locals);
        let motion = locals.map().get(&crate::motion::LOCAL_MOTION_SCHEME);
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
}

struct IndicatorLineNode {
    state: Rc<IndicatorState>,
    listener: Option<Rc<compose_ui::Listener>>,
    source_listened: Option<MutableInteractionSource>,
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
                locals: Cell::new(None),
                scope: Cell::new(None),
                color: RefCell::new(None),
                width: Animatable::new(self.0.unfocused_width),
            }),
            listener: None,
            source_listened: None,
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
        let locals = context.composition_locals();
        self.state.scope.set(Some(context.coroutine_scope()));
        self.state.locals.set(Some(locals));
        self.listen();
        let mut color = self.state.color.borrow_mut();
        if color.is_none() {
            let params = self.state.params.borrow();
            *color = Some(Animatable::new(self.state.colors(locals).indicator_color(
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
        if self.source_listened.as_ref() != Some(&self.state.params.borrow().source) {
            self.listen();
        }
        self.state.invalidate();
        context.invalidate_draw();
    }
    /// A new theme is read when it's wanted: the color at the next change, as Compose's; a shape
    /// of the theme's now.
    fn on_composition_locals_changed(&mut self, context: &mut NodeContext<'_>) {
        self.state.locals.set(Some(context.composition_locals()));
        context.invalidate_draw();
    }
}

impl DrawModifierNode for IndicatorLineNode {
    fn draw_after(&mut self, scope: &mut compose_ui::DrawScope<'_>) {
        let Some(color) = self.state.color.borrow().as_ref().map(Animatable::value) else {
            return;
        };
        let shape = self.state.params.borrow().shape.clone().or_else(|| {
            let shapes = self.state.locals.get()?.map().get(&crate::theme::LOCAL_SHAPES);
            Some(TextFieldDefaults::shape_of(&shapes))
        });
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
/// a placeholder while focused and empty, optional leading and trailing icons (in the minimum
/// touch target), a prefix and a suffix around the text (shown with the floated label), and
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
/// top edge the floated label sits on, cut around it, half its line above the field for it; with the
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
    /// The text as it shows, as Compose's `visualText`.
    visual_text: String,
    is_focused: bool,
    interaction_source: MutableInteractionSource,
    enabled: bool,
    is_error: bool,
    single_line: bool,
    label_position: TextFieldLabelPosition,
    slots: Slots,
    colors: TextFieldColors,
    content_padding: PaddingValues,
    shape: Shape,
    /// `None`, the default container, drawn without a composition for each of its animation's
    /// frames; else behind the rest, as big as they are.
    container: Option<Callback>,
}

/// As `minimizedLabelHalfHeight`: half a line of body small, whatever the font scale.
fn minimized_label_half_height() -> Dp {
    let line_height = MaterialTheme::typography().body_small.line_height;
    let line_height = if line_height.is_sp() { line_height } else { 16.sp() };
    let density = compose_ui::LOCAL_DENSITY.current();
    Dp(density.to_dp(density.sp_to_px(line_height)).0 / 2.0)
}

/// As `textFieldHorizontalIconPadding`: an icon is 24 dp in the minimum touch target, so that much
/// of the padding beside it is there already.
fn text_field_horizontal_icon_padding() -> Dp {
    let size = crate::components::LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.current();
    let size = if size.is_specified() { size.0 } else { 0.0 };
    Dp(((size - 24.0) / 2.0).max(0.0))
}

/// Whether the outline can be drawn as a rounded rectangle less a cut, with no clip and no
/// composition for a frame of its animations: one corner all round.
fn is_uniform(shape: &Shape) -> bool {
    match shape {
        Shape::Rectangle => true,
        Shape::Rounded(corners) | Shape::AbsoluteRounded(corners) => {
            corners.iter().all(|corner| *corner == corners[0])
        }
        Shape::Cut(_) | Shape::AbsoluteCut(_) | Shape::Custom(_) => false,
    }
}

/// The decoration of a field around its `inner_text_field`, as Compose's `CommonDecorationBox`: one
/// layout whatever's there, so a slot coming or going keeps the field and its focus.
fn decoration_box(parts: DecorationParts, inner_text_field: Callback) {
    let DecorationParts {
        variant,
        visual_text,
        is_focused,
        interaction_source,
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
    let empty = visual_text.is_empty();
    let has_label = slots.label.is_some();
    let shows_expanded = has_label && label_position.shows_expanded();
    let is_above = label_position.is_above();
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
    // An outline that isn't a rounded rectangle is a border, as `Container`'s, in a container of
    // its own.
    let container_slot = container_slot.or_else(|| {
        (variant == Variant::Outlined && !is_uniform(&shape)).then(|| {
            let (source, shape) = (interaction_source.clone(), shape.clone());
            compose_runtime::callback(move || {
                OutlinedTextFieldContainer!(
                    enabled,
                    is_error,
                    source.clone(),
                    modifier = Modifier,
                    colors = colors,
                    shape = shape.clone()
                );
            })
        })
    });
    let custom_container = container_slot.is_some();
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
    // As `animateBorderStrokeAsState`: disabled, it snaps. With the default container alone: one of
    // its own has its own.
    let indicator_state = (enabled && !custom_container).then(|| {
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
    let thickness_state = (enabled && !custom_container).then(|| {
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
    let typography = MaterialTheme::typography();
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
    let prefix_color = if enabled { colors.prefix_color } else { colors.disabled_prefix_color };
    let suffix_color = if enabled { colors.suffix_color } else { colors.disabled_suffix_color };
    let placeholder_color =
        if enabled { colors.placeholder_color } else { colors.disabled_placeholder_color };
    // As `TextFieldDefaults.Container`: the color read as it draws (`textFieldBackground`).
    let container_color = (!custom_container).then(|| {
        compose_animation::animate_color_as_state(
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
        )
    });
    // Shown while the field is empty and it's not transparent, as Compose's.
    let show_placeholder = empty && slots.placeholder.is_some() && show_placeholder_alpha;
    let (has_prefix, has_suffix) =
        (slots.prefix.is_some() && show_affix, slots.suffix.is_some() && show_affix);
    let minimized_half_height = minimized_label_half_height();
    let icon_padding = text_field_horizontal_icon_padding();
    // The label as last measured, for the cut, as `cutoutSize`: state, so the container, a node of
    // its own, draws again when it changes.
    let label_size = remember(|| mutable_state_of(compose_ui::Size::new(0.0, 0.0)));
    // The outline's path, reset each time it's drawn, as Compose's drawing caches keep one
    // (`CheckDrawingCache`): an animated cut redefines it in place on Android (D532).
    let outline = remember(Path::new);
    let line_path = remember(|| Rc::new(IndicatorPath::default()));
    let cut_path = remember(|| Rc::new(CutPath::default()));
    let is_cutout = has_label && label_position.is_cutout();
    let notch_alignment = label_position.minimized_alignment();
    // Where the cut goes, as `outlineCutout`: at the minimized label, as big as it is yet.
    let notch = move |size: compose_ui::Size,
                      density: compose_ui::Density,
                      direction: compose_ui::LayoutDirection| {
        if !is_cutout {
            return None;
        }
        let along = progress.map_or(0.0, |progress| progress.get());
        let label = label_size.get();
        let (width, height) = (label.width * along, label.height * along);
        if width <= 0.0 {
            return None;
        }
        let inner = density.to_px(4.dp());
        let ltr = direction == compose_ui::LayoutDirection::Ltr;
        let (start, end) =
            (density.to_px(content_padding.start), density.to_px(content_padding.end));
        let (left, right) = if ltr { (start, end) } else { (end, start) };
        let x = notch_alignment.align(
            width.round() as i32,
            (size.width - left - right).round() as i32,
            direction,
        ) as f32
            + left;
        Some(Notch {
            left: (x - inner).max(0.0),
            right: (x + width + inner).min(size.width),
            half_height: height / 2.0,
        })
    };
    let container_modifier = match (&container_slot, variant, container_color) {
        (None, Variant::Filled, Some(color)) => {
            let shape = shape.clone();
            let (line_path, cut_path) = (line_path.clone(), cut_path.clone());
            Modifier.draw_behind(move |scope| {
                let at = notch(scope.size(), scope.density(), scope.layout_direction());
                in_cut(scope, &cut_path, at, |scope| {
                    scope.draw_shape(&shape, color.get());
                    draw_indicator(
                        scope,
                        &line_path,
                        &shape,
                        indicator_thickness(),
                        indicator_color(),
                    );
                });
            })
        }
        (None, Variant::Outlined, Some(color)) => {
            let shape = shape.clone();
            let (outline, cut_path) = (outline.clone(), cut_path.clone());
            Modifier.draw_behind(move |scope| {
                let at = notch(scope.size(), scope.density(), scope.layout_direction());
                // Usually transparent: no clip for nothing.
                let fill = color.get();
                if fill.alpha() > 0 {
                    in_cut(scope, &cut_path, at, |scope| scope.draw_shape(&shape, fill));
                }
                draw_notched_outline(
                    scope,
                    &outline,
                    &shape,
                    indicator_thickness(),
                    indicator_color(),
                    at,
                );
            })
        }
        // The label's room out of what's given, as `outlineCutout`.
        _ if is_cutout => {
            let cut_path = cut_path.clone();
            Modifier.draw_with_content(move |scope| {
                let at = notch(scope.size(), scope.density(), scope.layout_direction());
                let Some(at) = at else {
                    scope.draw_content();
                    return;
                };
                let path = cut_path.path(scope, at);
                scope.save();
                scope.clip_outline(compose_ui::Outline::Generic(
                    path.data(),
                    compose_ui::Offset::ZERO,
                ));
                scope.draw_content();
                scope.restore();
            })
        }
        _ => Modifier,
    };
    let has_supporting = slots.supporting_text.is_some();
    let layout = DecorationLayout {
        has_leading: slots.leading_icon.is_some(),
        has_trailing: slots.trailing_icon.is_some(),
        has_label,
        has_placeholder: show_placeholder,
        has_prefix,
        has_suffix,
        has_supporting,
        single_line,
        position: label_position,
        content_padding,
        minimized_half_height,
        icon_padding,
        progress,
        label_size,
    };
    let policy =
        compose_runtime::remember_keyed(layout.key(), || Rc::new(layout) as Rc<dyn MeasurePolicy>);
    // Beside an icon, what the touch target doesn't already give, in dp, as Compose's.
    let beside = |padding: Dp, icon: bool| {
        if icon { Dp((padding.0 - icon_padding.0).max(0.0)) } else { padding }
    };
    let start_padding = beside(content_padding.start, slots.leading_icon.is_some());
    let end_padding = beside(content_padding.end, slots.trailing_icon.is_some());
    let label_min_height = compose_runtime::remember_keyed(progress.is_some(), move || {
        // As `textFieldLabelMinHeight`, the lerp read while measuring: no composition for a frame.
        Modifier.layout(move |scope, measurable, constraints| {
            let along = progress.map_or(1.0, |progress| progress.get());
            let min = scope.density().round_to_px(Dp(24.0 + (16.0 - 24.0) * along));
            let mut constraints = constraints;
            constraints.min_height = constraints.constrain_height(min);
            let placeable = scope.measure(measurable, constraints);
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        })
    });
    let body_large = typography.body_large.clone();
    let body_small = typography.body_small.clone();
    // Remembered: a keystroke composes the decoration, which is no reason to update the text's node.
    let text_sides = PaddingValues::new(
        if has_prefix { 0.dp() } else { start_padding },
        0.dp(),
        if has_suffix { 0.dp() } else { end_padding },
        0.dp(),
    );
    let text_padding =
        compose_runtime::remember_keyed(text_sides, move || {
            Modifier
                .height_in(MIN_TEXT_LINE_HEIGHT, Dp::UNSPECIFIED)
                .wrap_content_height()
                .padding((text_sides.start, 0.dp(), text_sides.end, 0.dp()))
        });
    let content = compose_runtime::callback(move || {
        // The text first: what focuses the field finds the first focus target below it, and Tab
        // goes from the text to its icons, as Compose's, whose field's target is above them
        // (D1298). As `visualTextSemantics`: with an affix, read between them.
        let inner = inner_text_field.clone();
        let text_modifier = if (has_prefix || has_suffix) && !empty {
            let visual_text = visual_text.clone();
            Modifier
                .semantics(move |config| config.set_text(visual_text.clone()))
                .then(text_padding.clone())
        } else {
            text_padding.clone()
        };
        Box(text_modifier, Alignment::TOP_START, true, compose_runtime::callback(move |_| inner()));
        // 2 dp between them and the text, as `PrefixSuffixTextPadding`.
        let affix = |content: &Option<Callback>, shows: bool, color: Color, padding: (Dp, Dp)| {
            let Some(content) = content.clone().filter(|_| shows) else { return };
            let style = body_large.clone();
            CompositionLocalProvider(
                (LOCAL_CONTENT_COLOR.provides(color), LOCAL_TEXT_STYLE.provides(style)),
                || {
                    let content = content.clone();
                    Box(
                        Modifier
                            .graphics_layer(move |layer| {
                                layer.alpha = affix_alpha.map_or(0.0, |alpha| alpha.get());
                            })
                            .height_in(MIN_TEXT_LINE_HEIGHT, Dp::UNSPECIFIED)
                            .wrap_content_height()
                            .padding((padding.0, 0.dp(), padding.1, 0.dp())),
                        Alignment::TOP_START,
                        false,
                        compose_runtime::callback(move |_| content()),
                    );
                },
            );
        };
        affix(&slots.prefix, has_prefix, prefix_color, (start_padding, 2.dp()));
        affix(&slots.suffix, has_suffix, suffix_color, (2.dp(), end_padding));
        // Each icon in the minimum touch target, centered.
        for (icon, color) in
            [(&slots.leading_icon, leading_color), (&slots.trailing_icon, trailing_color)]
        {
            if let Some(icon) = icon.clone() {
                CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
                    let icon = icon.clone();
                    Box(
                        crate::components::minimum_interactive_component_size(Modifier),
                        Alignment::CENTER,
                        false,
                        compose_runtime::callback(move |_| icon()),
                    );
                });
            }
        }
        if let (Some(label), Some(progress)) = (slots.label.clone(), progress) {
            let padding = if is_above {
                // As `AboveLabelHorizontalPadding` and `AboveLabelBottomPadding`.
                (4.dp(), 0.dp(), 4.dp(), 4.dp())
            } else if is_cutout {
                (0.dp(), 0.dp(), 0.dp(), 0.dp())
            } else {
                (start_padding, 0.dp(), end_padding, 0.dp())
            };
            let modifier = label_min_height.clone().wrap_content_height().padding(padding);
            FieldLabel(modifier, progress, label_color, is_focused, label);
        }
        if show_placeholder && let Some(placeholder) = slots.placeholder.clone() {
            let style = body_large.clone();
            let placeholder_modifier = Modifier
                .graphics_layer(move |layer| {
                    layer.alpha = placeholder_alpha.map_or(0.0, |alpha| alpha.get());
                })
                .then(text_padding.clone());
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
        if let Some(supporting_text) = slots.supporting_text.clone() {
            let style = body_small.clone();
            CompositionLocalProvider(
                (LOCAL_CONTENT_COLOR.provides(supporting_color), LOCAL_TEXT_STYLE.provides(style)),
                move || {
                    let supporting_text = supporting_text.clone();
                    // 16 dp in, 4 dp under the container, as `supportingTextPadding`.
                    Box(
                        Modifier
                            .height_in(MIN_SUPPORTING_TEXT_LINE_HEIGHT, Dp::UNSPECIFIED)
                            .wrap_content_height()
                            .padding((16.dp(), 4.dp(), 16.dp(), 0.dp())),
                        Alignment::TOP_START,
                        false,
                        compose_runtime::callback(move |_| supporting_text()),
                    );
                },
            );
        }
        // Behind everything by being placed first, and composed last so that no slot of the
        // decoration's other nodes moves when it changes.
        let container = container_slot.clone();
        Box(
            container_modifier.clone(),
            Alignment::TOP_START,
            true,
            compose_runtime::callback(move |_| {
                if let Some(container) = &container {
                    container();
                }
            }),
        );
    });
    compose_ui::Layout(Modifier, policy, content);
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
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    let is_focused = interaction_source.collect_is_focused_as_state().get();
    let visual_text = visual_text_of(&visual_transformation, &value);
    let has_label = slots.label.is_some();
    let cursor_color = if is_error { colors.error_cursor_color } else { colors.cursor_color };
    let text_color = if enabled { colors.text_color } else { colors.disabled_text_color };
    let style = TextStyle { color: text_style.color.or(text_color), ..text_style.clone() };
    let content_padding =
        content_padding.unwrap_or(default_content_padding(has_label, label_position));
    let decoration_source = interaction_source.clone();
    let decoration = move |inner_text_field: Callback| {
        decoration_box(
            DecorationParts {
                variant,
                visual_text: visual_text.clone(),
                is_focused,
                interaction_source: decoration_source.clone(),
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
    // Room above for a label on the edge, as `topPaddingForLabelCutout`, in the field's semantics.
    let modifier = if has_label && label_position.is_cutout() {
        modifier.semantics_with(true, |_| {}).padding((
            0.dp(),
            minimized_label_half_height(),
            0.dp(),
            0.dp(),
        ))
    } else {
        modifier
    };
    // In error, the field says so, as Compose's `semantics { error(…) }`.
    let modifier = if is_error {
        modifier.semantics(|config| config.error("Invalid input"))
    } else {
        modifier
    };
    let modifier =
        modifier.default_min_size(TextFieldDefaults::MIN_WIDTH, TextFieldDefaults::MIN_HEIGHT);
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
                Some(interaction_source),
                compose_runtime::callback(decoration),
            );
        },
    );
}

/// The text as it shows, as Compose's `visualText`.
fn visual_text_of(transformation: &VisualTransformation, value: &str) -> String {
    transformation.filter(value).map_or_else(|| value.to_string(), |transformed| transformed.text)
}

/// As `MinTextLineHeight`, `MinSupportingTextLineHeight`: small text keeps a line's height.
const MIN_TEXT_LINE_HEIGHT: Dp = Dp(24.0);
const MIN_SUPPORTING_TEXT_LINE_HEIGHT: Dp = Dp(16.0);

/// The room a label on the edge takes out of its container.
#[derive(Clone, Copy, PartialEq)]
struct Notch {
    left: f32,
    right: f32,
    half_height: f32,
}

/// All but a `Notch`, as a clip, for as long as it's the same: as `clipRect(…, ClipOp.Difference)`,
/// there being no difference clip here. Even-odd, so no `Path.op` on Android.
#[derive(Default)]
struct CutPath(RefCell<Option<((compose_ui::Size, Notch), Path)>>);

impl CutPath {
    fn path(&self, scope: &compose_ui::DrawScope<'_>, notch: Notch) -> Path {
        let size = scope.size();
        let mut cached = self.0.borrow_mut();
        if let Some((at, path)) = &*cached
            && *at == (size, notch)
        {
            return path.clone();
        }
        let path = Path::new();
        path.set_fill_type(compose_ui::PathFillType::EvenOdd);
        // Past the bounds, a drawing outside them not being cut by Compose's either.
        let far = size.width.max(size.height);
        path.add_rect(compose_ui::Rect::new(-far, -far, size.width + far, size.height + far));
        path.add_rect(compose_ui::Rect::new(
            notch.left,
            -notch.half_height,
            notch.right,
            notch.half_height,
        ));
        *cached = Some(((size, notch), path.clone()));
        path
    }
}

/// `draw` with the room for the label out of it, if there's one.
fn in_cut(
    scope: &mut compose_ui::DrawScope<'_>,
    cache: &CutPath,
    notch: Option<Notch>,
    draw: impl FnOnce(&mut compose_ui::DrawScope<'_>),
) {
    match notch {
        Some(notch) => {
            let path = cache.path(scope, notch);
            scope.clip_path(&path, draw);
        }
        None => draw(scope),
    }
}

#[derive(Clone, Copy, PartialEq)]
struct LayoutKey([bool; 8], TextFieldLabelPosition, PaddingValues, Dp, Dp);

impl compose_runtime::ComposeArg for LayoutKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The decoration's layout, as Compose's `TextFieldMeasurePolicy` (a label inside or above) and
/// `OutlinedTextFieldMeasurePolicy` (a label on the edge), whichever the field: its children, each
/// with its own padding, are the text, the leading and trailing icons, the prefix and suffix, the
/// label, the placeholder and the supporting text if they're there, and the container, placed
/// first and as big as the field less a label above and the supporting text.
struct DecorationLayout {
    has_leading: bool,
    has_trailing: bool,
    has_label: bool,
    has_placeholder: bool,
    has_prefix: bool,
    has_suffix: bool,
    has_supporting: bool,
    single_line: bool,
    position: TextFieldLabelPosition,
    content_padding: PaddingValues,
    minimized_half_height: Dp,
    icon_padding: Dp,
    /// The label's: read while measuring, each frame of it laying the field out.
    progress: Option<compose_runtime::State<f32>>,
    /// The label's size, for the cut.
    label_size: compose_runtime::MutableState<compose_ui::Size>,
}

/// A decoration's slots, as its children come.
struct DecorationSlots {
    text: Option<Measurable>,
    leading: Option<Measurable>,
    trailing: Option<Measurable>,
    prefix: Option<Measurable>,
    suffix: Option<Measurable>,
    label: Option<Measurable>,
    placeholder: Option<Measurable>,
    supporting: Option<Measurable>,
    container: Option<Measurable>,
}

/// What's measured first, whichever the label's place: each in what the ones before left.
struct Beside {
    leading: Option<Placeable>,
    trailing: Option<Placeable>,
    prefix: Option<Placeable>,
    suffix: Option<Placeable>,
    occupied: i32,
}

/// The heights `calculateHeight` goes by.
struct Heights {
    text: i32,
    label: i32,
    leading: i32,
    trailing: i32,
    prefix: i32,
    suffix: i32,
    placeholder: i32,
    supporting: i32,
}

fn width_of(placeable: &Option<Placeable>) -> i32 {
    placeable.as_ref().map_or(0, |placeable| placeable.width)
}

fn height_of(placeable: &Option<Placeable>) -> i32 {
    placeable.as_ref().map_or(0, |placeable| placeable.height)
}

/// As `lerp(Int, Int, Float)`.
fn lerp_int(start: i32, stop: i32, fraction: f32) -> i32 {
    start + round_half_up((stop - start) as f32 * fraction)
}

/// As `roundToInt`, half up whatever the sign.
fn round_half_up(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}

/// As `Alignment.CenterVertically.align`.
fn centered(size: i32, space: i32) -> i32 {
    round_half_up((space - size) as f32 / 2.0)
}

/// As `subtractConstraintSafely`.
fn less(width: i32, by: i32) -> i32 {
    if width == Constraints::INFINITY { width } else { (width - by).max(0) }
}

impl DecorationLayout {
    fn slots(&self, measurables: &[Measurable]) -> DecorationSlots {
        let mut children = measurables.iter().copied();
        let mut next = |present: bool| if present { children.next() } else { None };
        DecorationSlots {
            text: next(true),
            prefix: next(self.has_prefix),
            suffix: next(self.has_suffix),
            leading: next(self.has_leading),
            trailing: next(self.has_trailing),
            label: next(self.has_label),
            placeholder: next(self.has_placeholder),
            supporting: next(self.has_supporting),
            container: next(true),
        }
    }

    fn progress(&self) -> f32 {
        self.progress.map_or(1.0, |progress| progress.get())
    }

    /// As `calculateWidth`, before the constraints.
    fn width_of(
        &self,
        density: compose_ui::Density,
        [leading, trailing, prefix, suffix, text, label, placeholder]: [i32; 7],
    ) -> i32 {
        let affixes = prefix + suffix;
        if !self.position.is_cutout() {
            return leading + (text + affixes).max(placeholder + affixes).max(label) + trailing;
        }
        let progress = self.progress();
        let middle = (text + affixes).max(placeholder + affixes).max(lerp_int(label, 0, progress));
        let wrapped = leading + middle + trailing;
        let padding =
            density.to_px(self.content_padding.start) + density.to_px(self.content_padding.end);
        wrapped.max(round_half_up((label as f32 + padding) * progress))
    }

    /// As `calculateHeight`, before the constraints.
    fn height_of(&self, density: compose_ui::Density, heights: Heights) -> i32 {
        let is_above = self.position.is_above();
        let progress = self.progress();
        let input = heights
            .text
            .max(heights.placeholder)
            .max(heights.prefix)
            .max(heights.suffix)
            .max(if is_above { 0 } else { lerp_int(heights.label, 0, progress) });
        let padding = self.content_padding;
        let middle = if self.position.is_cutout() {
            let top = density.to_px(padding.top);
            let actual = top + (top.max(heights.label as f32 / 2.0) - top) * progress;
            round_half_up(actual + input as f32 + density.to_px(padding.bottom))
        } else {
            let vertical = density.round_to_px(Dp(padding.top.0 + padding.bottom.0));
            let not_overlapped = if heights.label > 0 && !is_above {
                // The label goes from over the text to above it, its part in the height with it;
                // a least of a minimized line keeps the height through the animation.
                density.round_to_px(Dp(self.minimized_half_height.0 * 2.0)).max(lerp_int(
                    0,
                    heights.label,
                    LABEL_HEIGHT_EASING.transform(progress),
                ))
            } else {
                0
            };
            vertical + not_overlapped + input
        };
        (if is_above { heights.label } else { 0 })
            + heights.leading.max(heights.trailing).max(middle)
            + heights.supporting
    }

    fn beside(
        &self,
        scope: &mut MeasureScope<'_>,
        slots: &DecorationSlots,
        loose: Constraints,
    ) -> Beside {
        let mut occupied = 0;
        let mut measure = |scope: &mut MeasureScope<'_>, slot: Option<Measurable>| {
            let placeable = slot.map(|slot| scope.measure(slot, loose.offset(-occupied, 0)));
            occupied += width_of(&placeable);
            placeable
        };
        let leading = measure(scope, slots.leading);
        let trailing = measure(scope, slots.trailing);
        let prefix = measure(scope, slots.prefix);
        let suffix = measure(scope, slots.suffix);
        Beside { leading, trailing, prefix, suffix, occupied }
    }

    /// As `intrinsicWidth`.
    fn intrinsic_width(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[Measurable],
        height: i32,
        max: bool,
    ) -> i32 {
        let slots = self.slots(measurables);
        let Some(text) = slots.text else { return 0 };
        let density = scope.density();
        let mut ask = |slot: Option<Measurable>| {
            slot.map_or(0, |slot| {
                if max {
                    scope.max_intrinsic_width(slot, height)
                } else {
                    scope.min_intrinsic_width(slot, height)
                }
            })
        };
        let widths = [
            ask(slots.leading),
            ask(slots.trailing),
            ask(slots.prefix),
            ask(slots.suffix),
            ask(Some(text)),
            ask(slots.label),
            ask(slots.placeholder),
        ];
        self.width_of(density, widths)
    }

    /// As `intrinsicHeight`: each at the width left by the ones before.
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
        let ask = |scope: &mut compose_ui::IntrinsicMeasureScope<'_>, slot, width| {
            if max {
                scope.max_intrinsic_height(slot, width)
            } else {
                scope.min_intrinsic_height(slot, width)
            }
        };
        let mut remaining = width;
        let mut icon = |scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
                        slot: Option<Measurable>| {
            slot.map_or(0, |slot| {
                remaining = less(remaining, scope.max_intrinsic_width(slot, Constraints::INFINITY));
                ask(scope, slot, width)
            })
        };
        let leading = icon(scope, slots.leading);
        let trailing = icon(scope, slots.trailing);
        // Floated on the edge, it's over the icons.
        let label_width = if self.position.is_cutout() {
            lerp_int(remaining.min(width), width, self.progress())
        } else {
            remaining
        };
        let label = slots.label.map_or(0, |slot| ask(scope, slot, label_width));
        let mut affix = |scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
                         slot: Option<Measurable>| {
            slot.map_or(0, |slot| {
                let height = ask(scope, slot, remaining);
                remaining = less(remaining, scope.max_intrinsic_width(slot, Constraints::INFINITY));
                height
            })
        };
        let prefix = affix(scope, slots.prefix);
        let suffix = affix(scope, slots.suffix);
        let heights = Heights {
            text: ask(scope, text, remaining),
            label,
            leading,
            trailing,
            prefix,
            suffix,
            placeholder: slots.placeholder.map_or(0, |slot| ask(scope, slot, remaining)),
            supporting: slots.supporting.map_or(0, |slot| ask(scope, slot, width)),
        };
        self.height_of(density, heights)
    }

    fn key(&self) -> LayoutKey {
        LayoutKey(
            [
                self.has_leading,
                self.has_trailing,
                self.has_label,
                self.has_placeholder,
                self.has_prefix,
                self.has_suffix,
                self.has_supporting,
                self.single_line,
            ],
            self.position,
            self.content_padding,
            self.minimized_half_height,
            self.icon_padding,
        )
    }

    /// As `TextFieldMeasurePolicy.measure`: a label inside or above.
    fn measure_inside(
        &self,
        scope: &mut MeasureScope<'_>,
        slots: DecorationSlots,
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let progress = self.progress();
        let top = density.round_to_px(self.content_padding.top);
        let bottom = density.round_to_px(self.content_padding.bottom);
        let loose = constraints.loosen();
        let beside = self.beside(scope, &slots, loose);
        let is_above = self.position.is_above();
        // Above, it's measured after the rest, its intrinsic height kept for it.
        let mut label = None;
        let mut label_intrinsic = 0;
        if is_above {
            label_intrinsic = slots
                .label
                .map_or(0, |slot| scope.min_intrinsic_height(slot, constraints.min_width));
        } else {
            label = slots
                .label
                .map(|slot| scope.measure(slot, loose.offset(-beside.occupied, -bottom)));
        }
        let supporting_intrinsic = slots
            .supporting
            .map_or(0, |slot| scope.min_intrinsic_height(slot, constraints.min_width));
        let effective_top = top + height_of(&label) + label_intrinsic;
        let mut text_constraints = constraints;
        text_constraints.min_height = 0;
        let text_constraints = text_constraints
            .offset(-beside.occupied, -effective_top - bottom - supporting_intrinsic);
        let Some(text_slot) = slots.text else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let text = scope.measure(text_slot, text_constraints);
        let mut placeholder_constraints = text_constraints;
        placeholder_constraints.min_width = 0;
        let placeholder =
            slots.placeholder.map(|slot| scope.measure(slot, placeholder_constraints));
        let occupied_vertically = height_of(&beside.leading)
            .max(height_of(&beside.trailing))
            .max(height_of(&beside.prefix))
            .max(height_of(&beside.suffix))
            .max(text.height.max(height_of(&placeholder)) + effective_top + bottom);
        let width = constraints.constrain_width(self.width_of(
            density,
            [
                width_of(&beside.leading),
                width_of(&beside.trailing),
                width_of(&beside.prefix),
                width_of(&beside.suffix),
                text.width,
                width_of(&label),
                width_of(&placeholder),
            ],
        ));
        if is_above {
            let mut label_constraints = loose;
            label_constraints.max_height = label_intrinsic;
            label_constraints.max_width = width;
            label = slots.label.map(|slot| scope.measure(slot, label_constraints));
        }
        let mut supporting_constraints = loose.offset(0, -occupied_vertically);
        supporting_constraints.min_height = 0;
        supporting_constraints.max_width = width;
        let supporting = slots.supporting.map(|slot| scope.measure(slot, supporting_constraints));
        let total_height = constraints.constrain_height(self.height_of(
            density,
            Heights {
                text: text.height,
                label: height_of(&label),
                leading: height_of(&beside.leading),
                trailing: height_of(&beside.trailing),
                prefix: height_of(&beside.prefix),
                suffix: height_of(&beside.suffix),
                placeholder: height_of(&placeholder),
                supporting: height_of(&supporting),
            },
        ));
        let y_offset = if is_above { height_of(&label) } else { 0 };
        let height = total_height - height_of(&supporting) - y_offset;
        if let Some(slot) = slots.container {
            let container = scope.measure(slot, Constraints::new(width, width, height, height));
            scope.place(&container, 0, y_offset);
        }
        let icon_y = |icon: &Placeable| y_offset + centered(icon.height, height);
        let leading_width = width_of(&beside.leading);
        let trailing_width = width_of(&beside.trailing);
        let text_x = leading_width + width_of(&beside.prefix);
        let text_y = |item: &Placeable| {
            if label.is_some() {
                // A label's place is above the text; above, the padding's what's between.
                y_offset + top + if is_above { 0 } else { height_of(&label) }
            } else if self.single_line {
                centered(item.height, height)
            } else {
                // Rounded as `(top.value * density).roundToInt()`.
                round_half_up(self.content_padding.top.0 * density.density)
            }
        };
        // In the order they're read (D1298 has an exception): the label, then what's in a line, as
        // Compose's; but the icons after, Tab going by the placement of the layouts its targets are
        // in, from the text to them, Compose's field's target being above them.
        if let Some(label) = &label {
            let direction = scope.layout_direction();
            let (x, y) = if is_above {
                (self.position.minimized_alignment().align(label.width, width, direction), 0)
            } else {
                let start_y = if self.single_line {
                    centered(label.height, height)
                } else {
                    // The padding given applies when it's minimized; more when it isn't.
                    top + density.round_to_px(self.minimized_half_height)
                };
                let left = if direction == compose_ui::LayoutDirection::Ltr {
                    leading_width
                } else {
                    trailing_width
                };
                let space = width - leading_width - trailing_width;
                let start_x =
                    self.position.expanded_alignment().align(label.width, space, direction) + left;
                let end_x =
                    self.position.minimized_alignment().align(label.width, space, direction) + left;
                (lerp_int(start_x, end_x, progress), lerp_int(start_y, top, progress))
            };
            // Not relative: the alignment has the direction.
            scope.place(label, x, y);
        }
        if let Some(prefix) = &beside.prefix {
            scope.place_relative(prefix, leading_width, text_y(prefix));
        }
        scope.place_relative(&text, text_x, text_y(&text));
        if let Some(placeholder) = &placeholder {
            scope.place_relative(placeholder, text_x, text_y(placeholder));
        }
        if let Some(suffix) = &beside.suffix {
            scope.place_relative(suffix, width - trailing_width - suffix.width, text_y(suffix));
        }
        if let Some(icon) = &beside.leading {
            scope.place_relative(icon, 0, icon_y(icon));
        }
        if let Some(icon) = &beside.trailing {
            scope.place_relative(icon, width - icon.width, icon_y(icon));
        }
        if let Some(supporting) = &supporting {
            scope.place_relative(supporting, 0, y_offset + height);
        }
        scope.layout(width, total_height)
    }

    /// As `OutlinedTextFieldMeasurePolicy.measure`: a label on the edge.
    fn measure_cutout(
        &self,
        scope: &mut MeasureScope<'_>,
        slots: DecorationSlots,
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let progress = self.progress();
        let padding = self.content_padding;
        let bottom = density.round_to_px(padding.bottom);
        let top_px = density.round_to_px(padding.top);
        let loose = constraints.loosen();
        let beside = self.beside(scope, &slots, loose);
        let horizontal = density.round_to_px(padding.start) + density.round_to_px(padding.end);
        let label_offset = lerp_int(beside.occupied + horizontal, horizontal, progress);
        let label =
            slots.label.map(|slot| scope.measure(slot, loose.offset(-label_offset, -bottom)));
        let label_size = compose_ui::Size::new(width_of(&label) as f32, height_of(&label) as f32);
        self.label_size.set(label_size);
        let supporting_intrinsic = slots
            .supporting
            .map_or(0, |slot| scope.min_intrinsic_height(slot, constraints.min_width));
        let top_padding = (height_of(&label) / 2).max(top_px);
        let mut text_constraints =
            constraints.offset(-beside.occupied, -bottom - top_padding - supporting_intrinsic);
        text_constraints.min_height = 0;
        let Some(text_slot) = slots.text else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let text = scope.measure(text_slot, text_constraints);
        let mut placeholder_constraints = text_constraints;
        placeholder_constraints.min_width = 0;
        let placeholder =
            slots.placeholder.map(|slot| scope.measure(slot, placeholder_constraints));
        let occupied_vertically = height_of(&beside.leading)
            .max(height_of(&beside.trailing))
            .max(height_of(&beside.prefix))
            .max(height_of(&beside.suffix))
            .max(text.height.max(height_of(&placeholder)) + top_padding + bottom);
        let width = constraints.constrain_width(self.width_of(
            density,
            [
                width_of(&beside.leading),
                width_of(&beside.trailing),
                width_of(&beside.prefix),
                width_of(&beside.suffix),
                text.width,
                width_of(&label),
                width_of(&placeholder),
            ],
        ));
        let mut supporting_constraints = loose.offset(0, -occupied_vertically);
        supporting_constraints.min_height = 0;
        supporting_constraints.max_width = width;
        let supporting = slots.supporting.map(|slot| scope.measure(slot, supporting_constraints));
        let total_height = constraints.constrain_height(self.height_of(
            density,
            Heights {
                text: text.height,
                label: height_of(&label),
                leading: height_of(&beside.leading),
                trailing: height_of(&beside.trailing),
                prefix: height_of(&beside.prefix),
                suffix: height_of(&beside.suffix),
                placeholder: height_of(&placeholder),
                supporting: height_of(&supporting),
            },
        ));
        let height = total_height - height_of(&supporting);
        if let Some(slot) = slots.container {
            let container = scope.measure(slot, Constraints::new(width, width, height, height));
            scope.place(&container, 0, 0);
        }
        let top = round_half_up(padding.top.0 * density.density);
        let leading_width = width_of(&beside.leading);
        let trailing_width = width_of(&beside.trailing);
        let half_label = height_of(&label) / 2;
        let y_of = |item: &Placeable| {
            let default = if self.single_line { centered(item.height, height) } else { top };
            default.max(half_label)
        };
        let text_x = leading_width + width_of(&beside.prefix);
        // In the order of `measure_inside`.
        if let Some(label) = &label {
            let start_y = if self.single_line { centered(label.height, height) } else { top };
            let y = lerp_int(start_y, -(label.height / 2), progress);
            let direction = scope.layout_direction();
            let ltr = direction == compose_ui::LayoutDirection::Ltr;
            let (start, end) = (padding.start.0 * density.density, padding.end.0 * density.density);
            let icon = self.icon_padding.0 * density.density;
            let leading_plus = match &beside.leading {
                None => start,
                Some(leading) => leading.width as f32 + (start - icon).max(0.0),
            };
            let trailing_plus = match &beside.trailing {
                None => end,
                Some(trailing) => trailing.width as f32 + (end - icon).max(0.0),
            };
            let left_padding = if ltr { start } else { end };
            let left_plus = if ltr { leading_plus } else { trailing_plus };
            let start_x = self.position.expanded_alignment().align(
                label.width,
                width - round_half_up(leading_plus + trailing_plus),
                direction,
            ) as f32
                + left_plus;
            let end_x = self.position.minimized_alignment().align(
                label.width,
                width - round_half_up(start + end),
                direction,
            ) as f32
                + left_padding;
            // Not relative: the alignment has the direction.
            scope.place(label, round_half_up(start_x + (end_x - start_x) * progress), y);
        }
        if let Some(prefix) = &beside.prefix {
            scope.place_relative(prefix, leading_width, y_of(prefix));
        }
        scope.place_relative(&text, text_x, y_of(&text));
        if let Some(placeholder) = &placeholder {
            scope.place_relative(placeholder, text_x, y_of(placeholder));
        }
        if let Some(suffix) = &beside.suffix {
            scope.place_relative(suffix, width - trailing_width - suffix.width, y_of(suffix));
        }
        if let Some(icon) = &beside.leading {
            scope.place_relative(icon, 0, centered(icon.height, height));
        }
        if let Some(icon) = &beside.trailing {
            scope.place_relative(icon, width - icon.width, centered(icon.height, height));
        }
        if let Some(supporting) = &supporting {
            scope.place_relative(supporting, 0, height);
        }
        scope.layout(width, total_height)
    }
}

impl MeasurePolicy for DecorationLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let slots = self.slots(measurables);
        if self.position.is_cutout() {
            self.measure_cutout(scope, slots, constraints)
        } else {
            self.measure_inside(scope, slots, constraints)
        }
    }

    // As Compose's `TextFieldMeasurePolicy` and `OutlinedTextFieldMeasurePolicy`: the slots'
    // intrinsics in the layout's arrangement, without measuring (D1207).
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

/// The outline of a uniform `shape` around the node, `thickness` wide, its top edge cut at `notch`,
/// as Compose's `outlineCutout` of a border: built in `path`, reset first.
fn draw_notched_outline(
    scope: &mut compose_ui::DrawScope<'_>,
    path: &Path,
    shape: &Shape,
    thickness: Dp,
    color: Color,
    notch: Option<Notch>,
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
    // From the cut's right end, clockwise round to its left end; corners as quarter circles.
    const K: f32 = 0.552_284_8;
    path.reset();
    path.move_to(notch.map_or(left + radius, |notch| notch.right), top);
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
    match notch {
        Some(notch) => path.line_to(notch.left, top),
        None => path.close(),
    }
    scope.draw_path(path, Paint::stroke(color, stroke));
}
