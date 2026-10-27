//! `Modifier`: an ordered chain of elements, each backed by a `ModifierNode` on its layout node
//! (docs/06-ui.md, "Modifiers"). Built-in elements are enum variants kept inline, so a chain of
//! up to four allocates nothing; user-defined modifiers are `Custom` elements.

use std::any::Any;
use std::fmt;
use std::rc::Rc;

use compose_runtime::{Callback, ComposeArg, IntoCallback};
use smallvec::SmallVec;

use crate::alignment::{Alignment, HorizontalAlignment, VerticalAlignment};
use crate::brush::Brush;
use crate::constraints::Constraints;
use crate::draw::DrawScope;
use crate::graphics::{Color, Shape};
use crate::input::{EventContext, KeyEvent, PointerEvent, PointerEventPass};
use crate::layout::{Measurable, MeasureResult, MeasureScope};
use crate::layout_modifiers::SizeOrIntrinsic;
use crate::semantics::{Role, SemanticsConfiguration};
use crate::text::SharedString;
use crate::unit::{Dp, DpSize, IntSize};

/// A chain of modifier elements, in call order: `Modifier.padding(8.dp()).background(…)` pads,
/// then draws the background inside the padding.
#[derive(Clone, Default)]
pub struct Modifier {
    elements: SmallVec<[Element; 6]>,
}

/// The empty modifier. `Modifier` is a type and a value, as in Kotlin, so code writes
/// `Modifier.padding(16.dp())`.
#[allow(non_upper_case_globals)]
pub const Modifier: Modifier = Modifier { elements: SmallVec::new_const() };

impl Modifier {
    pub(crate) fn elements(&self) -> &[Element] {
        &self.elements
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// This chain followed by `element`.
    pub(crate) fn then_element(mut self, element: Element) -> Modifier {
        self.elements.push(element);
        self
    }

    /// This chain followed by `other`, as `Modifier.then`.
    pub fn then(mut self, other: Modifier) -> Modifier {
        self.elements.extend(other.elements);
        self
    }

    /// A user-defined element, as `Modifier.then(element)`.
    pub fn then_custom(self, element: impl ModifierNodeElement) -> Modifier {
        self.then_element(Element::Custom(Rc::new(element)))
    }

    // ---- Layout -----------------------------------------------------------------------------

    /// Space around the content, as `Modifier.padding`'s overloads: `padding(8.dp())` on every
    /// side, `padding((16.dp(), 8.dp()))` horizontally and vertically, and
    /// `padding(PaddingValues { start: 8.dp(), ..PaddingValues::ZERO })` per side.
    pub fn padding(self, padding: impl Into<PaddingValues>) -> Modifier {
        self.padding_of(padding.into())
    }

    fn padding_of(self, padding: PaddingValues) -> Modifier {
        self.then_element(Element::Padding(padding))
    }

    /// Lays the content out at least `size` in both directions, centered, so small controls are
    /// easy to hit, as Material's `minimumInteractiveComponentSize()` (48 dp).
    pub fn minimum_touch_target(self, size: Dp) -> Modifier {
        self.then_element(Element::MinimumTouchTarget(size))
    }

    /// Moves the content by `x`, `y` without changing its size, as `Modifier.offset`.
    pub fn offset(self, x: Dp, y: Dp) -> Modifier {
        self.then_element(Element::Offset(x, y))
    }

    /// A size within the incoming constraints, as `Modifier.size`'s overloads: `size(40.dp())`
    /// square, `size((100.dp(), 50.dp()))` or a `DpSize` for a width and a height.
    pub fn size(self, size: impl Into<DpSize>) -> Modifier {
        let size = size.into();
        self.size_in(SizeSpec::exact(size.width, size.height, false))
    }

    /// A width within the incoming constraints, as `Modifier.width`'s overloads: in dp
    /// (`width(40.dp())`), or the content's intrinsic width (`width(IntrinsicSize::Max)`).
    pub fn width(self, width: impl Into<SizeOrIntrinsic>) -> Modifier {
        match width.into() {
            SizeOrIntrinsic::Size(width) => {
                self.size_in(SizeSpec::exact(width, Dp::UNSPECIFIED, false))
            }
            SizeOrIntrinsic::Intrinsic(size) => self.intrinsic_size(true, size, true),
        }
    }

    /// A height within the incoming constraints, as `Modifier.height`'s overloads (as `width`).
    pub fn height(self, height: impl Into<SizeOrIntrinsic>) -> Modifier {
        match height.into() {
            SizeOrIntrinsic::Size(height) => {
                self.size_in(SizeSpec::exact(Dp::UNSPECIFIED, height, false))
            }
            SizeOrIntrinsic::Intrinsic(size) => self.intrinsic_size(false, size, true),
        }
    }

    /// Exact sizes, overriding the incoming constraints, as `Modifier.requiredSize`'s overloads
    /// (as `size`).
    pub fn required_size(self, size: impl Into<DpSize>) -> Modifier {
        let size = size.into();
        self.size_in(SizeSpec::exact(size.width, size.height, true))
    }

    /// An exact width, overriding the incoming constraints, as `Modifier.requiredWidth`'s
    /// overloads: in dp, or the content's intrinsic width.
    pub fn required_width(self, width: impl Into<SizeOrIntrinsic>) -> Modifier {
        match width.into() {
            SizeOrIntrinsic::Size(width) => {
                self.size_in(SizeSpec::exact(width, Dp::UNSPECIFIED, true))
            }
            SizeOrIntrinsic::Intrinsic(size) => self.intrinsic_size(true, size, false),
        }
    }

    /// An exact height, overriding the incoming constraints, as `Modifier.requiredHeight`'s
    /// overloads (as `required_width`).
    pub fn required_height(self, height: impl Into<SizeOrIntrinsic>) -> Modifier {
        match height.into() {
            SizeOrIntrinsic::Size(height) => {
                self.size_in(SizeSpec::exact(Dp::UNSPECIFIED, height, true))
            }
            SizeOrIntrinsic::Intrinsic(size) => self.intrinsic_size(false, size, false),
        }
    }

    /// Bounds on the size, as `Modifier.sizeIn` (`Dp::UNSPECIFIED` for no bound).
    pub fn size_in(self, spec: SizeSpec) -> Modifier {
        self.then_element(Element::Size(spec))
    }

    /// Bounds on the width, within the incoming constraints, as `Modifier.widthIn`
    /// (`Dp::UNSPECIFIED` for no bound).
    pub fn width_in(self, min: Dp, max: Dp) -> Modifier {
        self.size_in(SizeSpec::bounds(min, Dp::UNSPECIFIED, max, Dp::UNSPECIFIED))
    }

    /// Bounds on the height, as `Modifier.heightIn`.
    pub fn height_in(self, min: Dp, max: Dp) -> Modifier {
        self.size_in(SizeSpec::bounds(Dp::UNSPECIFIED, min, Dp::UNSPECIFIED, max))
    }

    pub fn fill_max_width(self) -> Modifier {
        self.fill_max_width_with(1.0)
    }

    pub fn fill_max_height(self) -> Modifier {
        self.fill_max_height_with(1.0)
    }

    pub fn fill_max_size(self) -> Modifier {
        self.fill_max_size_with(1.0)
    }

    /// `fill_max_width` with its `fraction` of the maximum width, as
    /// `Modifier.fillMaxWidth(fraction)`.
    pub fn fill_max_width_with(self, fraction: f32) -> Modifier {
        self.then_element(Element::Fill(Fill { horizontal: Some(fraction), vertical: None }))
    }

    /// `fill_max_height` with its `fraction`, as `Modifier.fillMaxHeight(fraction)`.
    pub fn fill_max_height_with(self, fraction: f32) -> Modifier {
        self.then_element(Element::Fill(Fill { horizontal: None, vertical: Some(fraction) }))
    }

    /// `fill_max_size` with its `fraction`, as `Modifier.fillMaxSize(fraction)`.
    pub fn fill_max_size_with(self, fraction: f32) -> Modifier {
        self.then_element(Element::Fill(Fill {
            horizontal: Some(fraction),
            vertical: Some(fraction),
        }))
    }

    /// A share of the remaining space in a `Row` or `Column`, as `RowScope.weight`.
    pub fn weight(self, scope: impl WeightScope, weight: f32) -> Modifier {
        self.weight_with(scope, weight, true)
    }

    /// `weight` with `fill`: whether the child must take its whole share, as
    /// `Modifier.weight(weight, fill)`.
    pub fn weight_with(self, _scope: impl WeightScope, weight: f32, fill: bool) -> Modifier {
        assert!(weight > 0.0, "compose-ui: weight must be positive, was {weight}");
        // As Compose: an infinite weight is the largest finite one.
        self.then_element(Element::Weight(weight.min(f32::MAX), fill))
    }

    /// The child's alignment in its parent: a `BoxScope` takes an `Alignment`, a `RowScope` a
    /// `VerticalAlignment`, a `ColumnScope` a `HorizontalAlignment`.
    pub fn align<S: AlignScope>(self, _scope: S, alignment: S::Alignment) -> Modifier {
        self.then_element(Element::Align(S::parent_alignment(alignment)))
    }

    // ---- Drawing ----------------------------------------------------------------------------

    /// Fills the bounds behind the content with a color or a brush, as `Modifier.background`.
    pub fn background(self, brush: impl Into<Brush>) -> Modifier {
        self.background_of(brush.into(), Shape::Rectangle)
    }

    /// `background` in `shape`, as `Modifier.background(color, shape)` and `background(brush,
    /// shape)`; Compose's `alpha` is the brush's (`brush.with_alpha(alpha)`).
    pub fn background_with(self, brush: impl Into<Brush>, shape: Shape) -> Modifier {
        self.background_of(brush.into(), shape)
    }

    fn background_of(self, brush: Brush, shape: Shape) -> Modifier {
        match brush {
            Brush::Solid(color) => self.then_element(Element::Background(color, shape)),
            brush => self.then_custom(crate::draw_modifiers::BackgroundBrushElement {
                brush,
                shape,
                alpha: 1.0,
            }),
        }
    }

    /// A border of `width` and `color` around the bounds, as `Modifier.border(width, color)`.
    pub fn border(self, width: Dp, color: Color) -> Modifier {
        self.border_with(width, color, Shape::Rectangle)
    }

    /// `border` in `shape`, as `Modifier.border_with(width, color, shape)`.
    pub fn border_with(self, width: Dp, color: Color, shape: Shape) -> Modifier {
        self.then_element(Element::Border(width, color, shape))
    }

    /// Clips the content to `shape`, as `Modifier.clip`: a graphics layer clipping to its
    /// outline (a render node on Android), as Compose's `graphicsLayer(shape = shape, clip =
    /// true)`.
    pub fn clip(self, shape: Shape) -> Modifier {
        self.then_element(Element::Clip(shape))
    }

    /// Clips the content to the bounds, as `Modifier.clipToBounds`: a graphics layer clipping
    /// to a rectangle, as Compose's `graphicsLayer(clip = true)`.
    pub fn clip_to_bounds(self) -> Modifier {
        self.clip(Shape::Rectangle)
    }

    /// Draws (and hits) the element over its siblings of a lower `z_index`, under those of a
    /// higher one, as `Modifier.zIndex`: siblings of the same are in their placement order;
    /// several on one element add up.
    pub fn z_index(self, z_index: f32) -> Modifier {
        self.then_element(Element::ZIndex(z_index))
    }

    // ---- Input ------------------------------------------------------------------------------

    /// Calls `on_click` on a press released inside, and on Enter or Space when focused, as
    /// `Modifier.clickable`.
    pub fn clickable(self, on_click: impl IntoCallback<dyn Fn()>) -> Modifier {
        self.clickable_with(ClickableOptions::DEFAULT, on_click)
    }

    /// `clickable` with Compose's optional parameters (`enabled`, `role`, `indication`,
    /// `interaction_source`): `Modifier.clickable_with(ClickableOptions { role:
    /// Some(Role::Button), ..ClickableOptions::DEFAULT }, on_click)`.
    pub fn clickable_with(
        self,
        options: ClickableOptions,
        on_click: impl IntoCallback<dyn Fn()>,
    ) -> Modifier {
        self.then_element(Element::Clickable(Clickable {
            enabled: options.enabled,
            role: options.role,
            indication: options.indication,
            interaction_source: options.interaction_source,
            on_click: on_click.into_callback(),
        }))
    }

    /// A toggle: a click calls `on_value_change(!value)`, as `Modifier.toggleable`.
    pub fn toggleable(
        self,
        value: bool,
        on_value_change: impl IntoCallback<dyn Fn(bool)>,
    ) -> Modifier {
        self.toggleable_with(value, ClickableOptions::DEFAULT, on_value_change)
    }

    /// `toggleable` with Compose's optional parameters, as `clickable_with`.
    pub fn toggleable_with(
        self,
        value: bool,
        options: ClickableOptions,
        on_value_change: impl IntoCallback<dyn Fn(bool)>,
    ) -> Modifier {
        self.then_element(Element::Toggleable(Toggleable {
            value,
            enabled: options.enabled,
            role: options.role,
            indication: options.indication,
            interaction_source: options.interaction_source,
            on_value_change: on_value_change.into_callback(),
        }))
    }

    /// A selectable item (a radio button, a tab): a click calls `on_click`, and its semantics
    /// say whether it's `selected`, as `Modifier.selectable`.
    pub fn selectable(self, selected: bool, on_click: impl IntoCallback<dyn Fn()>) -> Modifier {
        self.selectable_with(selected, ClickableOptions::DEFAULT, on_click)
    }

    /// `selectable` with Compose's optional parameters, as `clickable_with`.
    pub fn selectable_with(
        self,
        selected: bool,
        options: ClickableOptions,
        on_click: impl IntoCallback<dyn Fn()>,
    ) -> Modifier {
        self.semantics(|config| config.selected = Some(selected)).clickable_with(options, on_click)
    }

    /// Groups selectable items (radio buttons) for accessibility, as
    /// `Modifier.selectableGroup`.
    pub fn selectable_group(self) -> Modifier {
        self.semantics(|config| config.selectable_group = true)
    }

    /// A click, a double click and a long press, as `Modifier.combinedClickable`: with
    /// `on_double_click`, a click waits for the double-click timeout (300 ms) before calling
    /// `on_click`; with `on_long_click`, holding past the long-press timeout (400 ms) calls it
    /// instead, with `LOCAL_HAPTIC_FEEDBACK`'s `LongPress` feedback (read when made in
    /// composition). Enter and Space click when focused.
    pub fn combined_clickable(
        self,
        on_long_click: Option<Callback<dyn Fn()>>,
        on_double_click: Option<Callback<dyn Fn()>>,
        on_click: impl IntoCallback<dyn Fn()>,
    ) -> Modifier {
        self.combined_clickable_with(
            ClickableOptions::DEFAULT,
            on_long_click,
            on_double_click,
            on_click,
        )
    }

    /// `combined_clickable` with Compose's optional parameters, as `clickable_with`.
    pub fn combined_clickable_with(
        self,
        options: ClickableOptions,
        on_long_click: Option<Callback<dyn Fn()>>,
        on_double_click: Option<Callback<dyn Fn()>>,
        on_click: impl IntoCallback<dyn Fn()>,
    ) -> Modifier {
        self.then_custom(crate::nodes::CombinedClickableElement {
            clickable: Clickable {
                enabled: options.enabled,
                role: options.role,
                indication: options.indication,
                interaction_source: options.interaction_source,
                on_click: on_click.into_callback(),
            },
            // A long click vibrates, as Compose's (`hapticFeedbackEnabled`, on by default).
            haptic_feedback: on_long_click
                .as_ref()
                .and_then(|_| crate::platform_services::current_haptic_feedback()),
            on_long_click,
            on_double_click,
        })
    }

    /// Reports to `interaction_source` whether a mouse is over the element, as
    /// `Modifier.hoverable`.
    pub fn hoverable(
        self,
        interaction_source: crate::interaction::MutableInteractionSource,
    ) -> Modifier {
        self.hoverable_with(interaction_source, true)
    }

    /// `hoverable` with `enabled`, as `Modifier.hoverable_with(interactionSource, enabled)`.
    pub fn hoverable_with(
        self,
        interaction_source: crate::interaction::MutableInteractionSource,
        enabled: bool,
    ) -> Modifier {
        self.then_custom(crate::nodes::HoverableElement { interaction_source, enabled })
    }

    /// Makes the element a focus target, as `Modifier.focusable()`.
    pub fn focusable(self) -> Modifier {
        self.focusable_with(true)
    }

    /// `focusable` with `enabled`, as `Modifier.focusable_with(enabled)` (no interaction source yet).
    pub fn focusable_with(self, enabled: bool) -> Modifier {
        self.then_element(Element::Focusable(enabled))
    }

    /// Calls `on_focus_changed` when the focus target after it in the chain gains or loses
    /// focus, as `Modifier.onFocusChanged`.
    pub fn on_focus_changed(self, on_focus_changed: impl IntoCallback<dyn Fn(bool)>) -> Modifier {
        self.then_element(Element::OnFocusChanged(on_focus_changed.into_callback()))
    }

    /// Handles key events sent to the focused element or its descendants; returning true
    /// consumes the event, as `Modifier.onKeyEvent`.
    pub fn on_key_event(
        self,
        on_key_event: impl IntoCallback<dyn Fn(KeyEvent) -> bool>,
    ) -> Modifier {
        self.then_element(Element::OnKeyEvent(on_key_event.into_callback()))
    }

    // ---- Semantics --------------------------------------------------------------------------

    /// A tag tests find the element by, as `Modifier.testTag`.
    pub fn test_tag(self, tag: impl Into<SharedString>) -> Modifier {
        self.then_element(Element::TestTag(tag.into()))
    }

    /// Semantics properties, as `Modifier.semantics`; `merge_descendants` merges the
    /// descendants' semantics into this node, as a button merges its label. `properties` runs
    /// at once and its result is kept as a value, so an unchanged modifier stays `same`.
    pub fn semantics(self, properties: impl FnOnce(&mut SemanticsConfiguration)) -> Modifier {
        self.semantics_with(false, properties)
    }

    /// `semantics` with `merge_descendants`, as `Modifier.semantics(mergeDescendants)`.
    pub fn semantics_with(
        self,
        merge_descendants: bool,
        properties: impl FnOnce(&mut SemanticsConfiguration),
    ) -> Modifier {
        let mut config = SemanticsConfiguration::default();
        properties(&mut config);
        self.then_element(Element::Semantics(merge_descendants, Rc::new(config)))
    }

    /// Replaces the descendants' semantics with `properties`, as `Modifier.clearAndSetSemantics`:
    /// what's inside reads as nothing (an icon next to its label).
    pub fn clear_and_set_semantics(
        self,
        properties: impl FnOnce(&mut SemanticsConfiguration),
    ) -> Modifier {
        self.semantics(|config| {
            properties(config);
            config.clear_descendants = true;
        })
    }
}

/// A modifier of one user-defined element shared by reference: for a toolkit whose element
/// must stay the same object across recompositions (foundation's text field).
pub fn shared_custom_modifier(element: Rc<dyn AnyModifierNodeElement>) -> Modifier {
    Modifier.then_element(Element::Custom(element))
}

impl ComposeArg for Modifier {
    fn same(&self, other: &Self) -> bool {
        self.elements.len() == other.elements.len()
            && self.elements.iter().zip(&other.elements).all(|(a, b)| a.same(b))
    }
}

impl fmt::Debug for Modifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Modifier")?;
        for element in &self.elements {
            write!(formatter, ".{element:?}")?;
        }
        Ok(())
    }
}

/// What `clickable_with`, `toggleable_with`, `selectable_with` and `combined_clickable_with`
/// take beyond their required parameters: Compose's optional ones, built by struct update from
/// `DEFAULT` (D261, D263).
#[derive(Clone, PartialEq)]
pub struct ClickableOptions {
    /// Whether it reacts to input (and says so to accessibility), as `enabled`.
    pub enabled: bool,
    /// What it is to accessibility, as `role`.
    pub role: Option<Role>,
    /// The state layer drawn while pressed or focused (Material's ripple, without its
    /// animation), as `indication`; `Indication::NONE` draws none.
    pub indication: Indication,
    /// Where its press is reported, as `interactionSource`: with `Indication::NONE`, something
    /// else shows the press.
    pub interaction_source: Option<crate::interaction::MutableInteractionSource>,
}

impl ClickableOptions {
    /// Compose's defaults: enabled, no role, no indication, no interaction source.
    pub const DEFAULT: ClickableOptions = ClickableOptions {
        enabled: true,
        role: None,
        indication: Indication::NONE,
        interaction_source: None,
    };
}

impl Default for ClickableOptions {
    fn default() -> Self {
        ClickableOptions::DEFAULT
    }
}

/// Padding in `Dp`, as `PaddingValues`: `PaddingValues::from(8.dp())` on every side,
/// `PaddingValues::from((16.dp(), 8.dp()))` horizontally and vertically, or per side by struct
/// update, `PaddingValues { start: 8.dp(), ..PaddingValues::ZERO }` (Compose's zero defaults).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct PaddingValues {
    pub start: Dp,
    pub top: Dp,
    pub end: Dp,
    pub bottom: Dp,
}

impl PaddingValues {
    /// No padding.
    pub const ZERO: PaddingValues = PaddingValues::new(Dp(0.0), Dp(0.0), Dp(0.0), Dp(0.0));

    pub const fn new(start: Dp, top: Dp, end: Dp, bottom: Dp) -> PaddingValues {
        PaddingValues { start, top, end, bottom }
    }

    pub const fn all(all: Dp) -> PaddingValues {
        PaddingValues::new(all, all, all, all)
    }

    pub const fn symmetric(horizontal: Dp, vertical: Dp) -> PaddingValues {
        PaddingValues::new(horizontal, vertical, horizontal, vertical)
    }
}

impl From<Dp> for PaddingValues {
    /// The same padding on every side, as `PaddingValues(all)`.
    fn from(all: Dp) -> PaddingValues {
        PaddingValues::all(all)
    }
}

impl From<(Dp, Dp, Dp, Dp)> for PaddingValues {
    /// Start, top, end and bottom padding, as `PaddingValues(start, top, end, bottom)`.
    fn from((start, top, end, bottom): (Dp, Dp, Dp, Dp)) -> PaddingValues {
        PaddingValues::new(start, top, end, bottom)
    }
}

impl From<(Dp, Dp)> for PaddingValues {
    /// Horizontal and vertical padding, as `PaddingValues(horizontal, vertical)`.
    fn from((horizontal, vertical): (Dp, Dp)) -> PaddingValues {
        PaddingValues::symmetric(horizontal, vertical)
    }
}

impl ComposeArg for PaddingValues {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Size bounds in `Dp`, `Dp::UNSPECIFIED` meaning no bound, as `Modifier.sizeIn`. `enforce`
/// keeps them even outside the incoming constraints, as the `required…` modifiers.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SizeSpec {
    pub min_width: Dp,
    pub min_height: Dp,
    pub max_width: Dp,
    pub max_height: Dp,
    pub enforce_incoming: bool,
}

impl SizeSpec {
    pub const fn exact(width: Dp, height: Dp, required: bool) -> SizeSpec {
        SizeSpec {
            min_width: width,
            min_height: height,
            max_width: width,
            max_height: height,
            enforce_incoming: !required,
        }
    }

    pub const fn bounds(min_width: Dp, min_height: Dp, max_width: Dp, max_height: Dp) -> SizeSpec {
        SizeSpec { min_width, min_height, max_width, max_height, enforce_incoming: true }
    }
}

/// `fill_max_*` fractions, per axis.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Fill {
    pub horizontal: Option<f32>,
    pub vertical: Option<f32>,
}

/// A child's alignment in its parent, as the scoped `align` modifiers set it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ParentAlignment {
    Box(Alignment),
    Horizontal(HorizontalAlignment),
    Vertical(VerticalAlignment),
}

/// The data a child gives its parent's measure policy, as Compose's parent data modifiers.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ParentData {
    /// The weight and fill of `weight`.
    pub weight: Option<(f32, bool)>,
    pub alignment: Option<ParentAlignment>,
}

/// A scope whose children take a `weight`: `RowScope` and `ColumnScope`.
pub trait WeightScope: Copy {}

/// A scope whose children take an `align`.
pub trait AlignScope: Copy {
    type Alignment;
    fn parent_alignment(alignment: Self::Alignment) -> ParentAlignment;
}

/// The content scope of a `Row`: a token only a `Row` makes, so its scoped modifiers can't be
/// used elsewhere.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RowScope(());

/// The content scope of a `Column`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ColumnScope(());

/// The content scope of a `Box`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BoxScope(());

impl RowScope {
    /// For `Row`'s implementation only.
    #[doc(hidden)]
    pub const fn __new() -> RowScope {
        RowScope(())
    }
}

impl ColumnScope {
    #[doc(hidden)]
    pub const fn __new() -> ColumnScope {
        ColumnScope(())
    }
}

impl BoxScope {
    #[doc(hidden)]
    pub const fn __new() -> BoxScope {
        BoxScope(())
    }
}

impl WeightScope for RowScope {}
impl WeightScope for ColumnScope {}

impl AlignScope for RowScope {
    type Alignment = VerticalAlignment;
    fn parent_alignment(alignment: VerticalAlignment) -> ParentAlignment {
        ParentAlignment::Vertical(alignment)
    }
}

impl AlignScope for ColumnScope {
    type Alignment = HorizontalAlignment;
    fn parent_alignment(alignment: HorizontalAlignment) -> ParentAlignment {
        ParentAlignment::Horizontal(alignment)
    }
}

impl AlignScope for BoxScope {
    type Alignment = Alignment;
    fn parent_alignment(alignment: Alignment) -> ParentAlignment {
        ParentAlignment::Box(alignment)
    }
}

macro_rules! scopes_compose_arg {
    ($($type:ty),*) => {
        $(impl ComposeArg for $type {
            fn same(&self, _: &Self) -> bool {
                true
            }
        })*
    };
}

scopes_compose_arg!(RowScope, ColumnScope, BoxScope);

/// A state layer shown while pressed, focused or hovered, as Material's ripple (without its
/// animation): `color` over the node's bounds, or, `unbounded`, a circle of `radius` centered
/// on it, as `ripple(bounded = false, radius)`. An unspecified color shows nothing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Indication {
    pub color: Color,
    /// The circle's radius, for an unbounded one.
    pub radius: Option<crate::unit::Dp>,
}

impl Indication {
    /// None, as `indication = null`.
    pub const NONE: Indication = Indication { color: Color::UNSPECIFIED, radius: None };

    /// Over the node's bounds, as `ripple()`.
    pub fn bounded(color: Color) -> Indication {
        Indication { color, radius: None }
    }

    /// A circle of `radius` centered on the node, as `ripple(bounded = false, radius)`.
    pub fn unbounded(color: Color, radius: crate::unit::Dp) -> Indication {
        Indication { color, radius: Some(radius) }
    }
}

impl From<Color> for Indication {
    fn from(color: Color) -> Indication {
        Indication::bounded(color)
    }
}

/// `clickable`'s data.
#[derive(Clone)]
pub(crate) struct Clickable {
    pub enabled: bool,
    pub role: Option<Role>,
    /// The state layer, drawn at 10% while pressed or focused (Material's indication).
    pub indication: Indication,
    /// Where its press is reported.
    pub interaction_source: Option<crate::interaction::MutableInteractionSource>,
    pub on_click: Callback<dyn Fn()>,
}

/// `toggleable`'s data.
#[derive(Clone)]
pub(crate) struct Toggleable {
    pub value: bool,
    pub enabled: bool,
    pub role: Option<Role>,
    /// As `Clickable::indication`.
    pub indication: Indication,
    /// As `Clickable::interaction_source`.
    pub interaction_source: Option<crate::interaction::MutableInteractionSource>,
    pub on_value_change: Callback<dyn Fn(bool)>,
}

/// One modifier element: plain data, compared on recomposition to update its node in place.
#[derive(Clone)]
pub(crate) enum Element {
    Padding(PaddingValues),
    Offset(Dp, Dp),
    MinimumTouchTarget(Dp),
    Size(SizeSpec),
    Fill(Fill),
    Weight(f32, bool),
    Align(ParentAlignment),
    Background(Color, Shape),
    Border(Dp, Color, Shape),
    Clip(Shape),
    Clickable(Clickable),
    Toggleable(Toggleable),
    Focusable(bool),
    OnFocusChanged(Callback<dyn Fn(bool)>),
    OnKeyEvent(Callback<dyn Fn(KeyEvent) -> bool>),
    TestTag(SharedString),
    Semantics(bool, Rc<SemanticsConfiguration>),
    /// `z_index`: the order among siblings, read from the elements as parent data.
    ZIndex(f32),
    Custom(Rc<dyn AnyModifierNodeElement>),
}

impl Element {
    /// Whether `other` is the same kind of element, so its node can be updated in place.
    pub(crate) fn same_kind(&self, other: &Element) -> bool {
        match (self, other) {
            (Element::Custom(a), Element::Custom(b)) => (**a).type_id() == (**b).type_id(),
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }

    /// Whether `other` is equal: then its node needs no update.
    pub(crate) fn same(&self, other: &Element) -> bool {
        use Element::*;
        match (self, other) {
            (Padding(a), Padding(b)) => a == b,
            (Offset(a, b), Offset(c, d)) => a == c && b == d,
            (MinimumTouchTarget(a), MinimumTouchTarget(b)) => a == b,
            (Size(a), Size(b)) => a == b,
            (Fill(a), Fill(b)) => a == b,
            (Weight(a, b), Weight(c, d)) => a == c && b == d,
            (Align(a), Align(b)) => a == b,
            (Background(a, b), Background(c, d)) => a == c && b == d,
            (Border(a, b, c), Border(d, e, f)) => a == d && b == e && c == f,
            (Clip(a), Clip(b)) => a == b,
            (Clickable(a), Clickable(b)) => {
                a.enabled == b.enabled
                    && a.role == b.role
                    && a.indication == b.indication
                    && a.interaction_source == b.interaction_source
                    && a.on_click.ptr_eq(&b.on_click)
            }
            (Toggleable(a), Toggleable(b)) => {
                a.value == b.value
                    && a.enabled == b.enabled
                    && a.role == b.role
                    && a.indication == b.indication
                    && a.on_value_change.ptr_eq(&b.on_value_change)
            }
            (Focusable(a), Focusable(b)) => a == b,
            (OnFocusChanged(a), OnFocusChanged(b)) => a.ptr_eq(b),
            (OnKeyEvent(a), OnKeyEvent(b)) => a.ptr_eq(b),
            (TestTag(a), TestTag(b)) => a.same(b),
            (Semantics(a, b), Semantics(c, d)) => a == c && (Rc::ptr_eq(b, d) || b == d),
            (ZIndex(a), ZIndex(b)) => a == b,
            (Custom(a), Custom(b)) => Rc::ptr_eq(a, b) || a.same_dyn(&**b),
            _ => false,
        }
    }
}

impl fmt::Debug for Element {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Element::*;
        match self {
            Padding(padding) => write!(
                formatter,
                "padding({:?}, {:?}, {:?}, {:?})",
                padding.start, padding.top, padding.end, padding.bottom
            ),
            Offset(x, y) => write!(formatter, "offset({x:?}, {y:?})"),
            MinimumTouchTarget(size) => write!(formatter, "minimum_touch_target({size:?})"),
            Size(spec) => {
                let bound =
                    |dp: Dp| if dp.is_specified() { format!("{dp:?}") } else { "_".to_string() };
                let axis = |min: Dp, max: Dp| {
                    if min == max || (!min.is_specified() && !max.is_specified()) {
                        bound(min)
                    } else {
                        format!("{}..{}", bound(min), bound(max))
                    }
                };
                write!(
                    formatter,
                    "{}size({} x {})",
                    if spec.enforce_incoming { "" } else { "required_" },
                    axis(spec.min_width, spec.max_width),
                    axis(spec.min_height, spec.max_height)
                )
            }
            Fill(fill) => match (fill.horizontal, fill.vertical) {
                (Some(h), Some(v)) if h == v => write!(formatter, "fill_max_size({h})"),
                (Some(h), None) => write!(formatter, "fill_max_width({h})"),
                (None, Some(v)) => write!(formatter, "fill_max_height({v})"),
                (h, v) => write!(formatter, "fill({h:?}, {v:?})"),
            },
            Weight(weight, fill) => write!(formatter, "weight({weight}, {fill})"),
            Align(alignment) => write!(formatter, "align({alignment:?})"),
            Background(color, shape) => write!(formatter, "background({color:?}, {shape:?})"),
            Border(width, color, shape) => {
                write!(formatter, "border({width:?}, {color:?}, {shape:?})")
            }
            Clip(shape) => write!(formatter, "clip({shape:?})"),
            Clickable(clickable) => write!(formatter, "clickable(enabled = {})", clickable.enabled),
            Toggleable(toggleable) => write!(
                formatter,
                "toggleable({}, enabled = {})",
                toggleable.value, toggleable.enabled
            ),
            Focusable(enabled) => write!(formatter, "focusable({enabled})"),
            OnFocusChanged(_) => formatter.write_str("on_focus_changed"),
            OnKeyEvent(_) => formatter.write_str("on_key_event"),
            TestTag(tag) => write!(formatter, "test_tag({tag:?})"),
            Semantics(merge, _) => write!(formatter, "semantics(merge = {merge})"),
            ZIndex(z_index) => write!(formatter, "z_index({z_index})"),
            Custom(element) => {
                let mut info = InspectorInfo::default();
                element.inspect_dyn(&mut info);
                let name = info.name.unwrap_or_else(|| element.name_dyn());
                write!(formatter, "{}", crate::tree::short_type_name(name))?;
                match info.value {
                    Some(value) => write!(formatter, "({value})"),
                    None => Ok(()),
                }
            }
        }
    }
}

// ---- Modifier.Node ---------------------------------------------------------------------------

/// A user-defined modifier element, as Compose's `ModifierNodeElement<N>`: it creates its node,
/// and updates it in place when a new element of the same type replaces it. Added to a chain
/// with `Modifier.then_custom(element)`.
pub trait ModifierNodeElement: 'static {
    /// The node it creates and updates.
    type Node: ModifierNode;
    fn create(&self) -> Self::Node;
    /// Updates `node`, which an element of this type created (Compose's `update(node: N)`). An
    /// update then invalidates what the node's kinds say (a layout node is measured again, a
    /// draw node drawn again), unless `ModifierNode::should_auto_invalidate` says not to.
    fn update(&self, node: &mut Self::Node);
    /// Whether `other` is equal, so the node needs no update (Compose's `equals`).
    fn same(&self, other: &Self) -> bool;
    /// A name for dumps.
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
    /// What inspection and dumps show of it, as `InspectorInfo.inspectableProperties()`: by
    /// default its name.
    fn inspectable_properties(&self, info: &mut InspectorInfo) {
        info.name = Some(self.name());
    }
}

/// What an element shows inspection and dumps, as Compose's `InspectorInfo`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InspectorInfo {
    pub name: Option<&'static str>,
    pub value: Option<String>,
    pub properties: Vec<(&'static str, String)>,
}

/// The object-safe form of `ModifierNodeElement` a chain stores, implemented for every element:
/// what the chain diff calls without knowing the element's type.
pub trait AnyModifierNodeElement: Any {
    fn create_boxed(&self) -> Box<dyn ModifierNode>;
    /// Updates `node`, which an element of the same type created.
    fn update_dyn(&self, node: &mut dyn ModifierNode);
    /// Whether `other` is an equal element of the same type.
    fn same_dyn(&self, other: &dyn AnyModifierNodeElement) -> bool;
    fn name_dyn(&self) -> &'static str;
    fn inspect_dyn(&self, info: &mut InspectorInfo);
}

impl<E: ModifierNodeElement> AnyModifierNodeElement for E {
    fn create_boxed(&self) -> Box<dyn ModifierNode> {
        Box::new(self.create())
    }

    fn update_dyn(&self, node: &mut dyn ModifierNode) {
        let node = node.downcast_mut::<E::Node>();
        debug_assert!(node.is_some(), "compose-ui: an element updates the node its type created");
        if let Some(node) = node {
            self.update(node);
        }
    }

    fn same_dyn(&self, other: &dyn AnyModifierNodeElement) -> bool {
        (other as &dyn Any).downcast_ref::<E>().is_some_and(|other| self.same(other))
    }

    fn name_dyn(&self) -> &'static str {
        self.name()
    }

    fn inspect_dyn(&self, info: &mut InspectorInfo) {
        self.inspectable_properties(info);
    }
}

impl dyn AnyModifierNodeElement {
    /// The element as `T`, if it is one.
    pub fn downcast_ref<T: ModifierNodeElement>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}

/// A modifier's node: the state an element keeps across recompositions (pressed, focused, a
/// laid-out paragraph), and its capabilities, as Compose's `Modifier.Node` delegates.
pub trait ModifierNode: Any {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        None
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        None
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        None
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        None
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        None
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        None
    }
    /// Whether the node is a repaint boundary: what it draws, its children included, is a
    /// group of the display list a renderer may keep while it's unchanged (graphics layers,
    /// `Modifier.repaint_boundary`).
    fn is_repaint_boundary(&self) -> bool {
        false
    }
    /// Told where it is after each layout that moved or resized it, as
    /// `GlobalPositionAwareModifierNode` (`on_globally_positioned`).
    fn as_global_position_aware(
        &mut self,
    ) -> Option<&mut dyn crate::layout_callbacks::GlobalPositionAwareModifierNode> {
        None
    }
    /// A text a `SelectionContainer` selects in.
    fn as_selectable(&mut self) -> Option<&mut dyn crate::text_selection::Selectable> {
        None
    }
    /// The shape this node clips its content to, for hit testing: what's outside isn't hit
    /// (`clip`, a scroll's viewport), as a Compose graphics layer clips.
    fn hit_clip(&self) -> Option<crate::graphics::Shape> {
        None
    }
    /// [`hit_clip`](ModifierNode::hit_clip) for a node of `size` at `density` (a graphics
    /// layer's block may set its clip from them): what hit testing asks.
    fn hit_clip_at(
        &self,
        size: crate::unit::Size,
        density: crate::unit::Density,
    ) -> Option<crate::graphics::Shape> {
        let _ = (size, density);
        self.hit_clip()
    }
    /// Whether this pointer input node only watches the events passing to what's hit inside
    /// it: it doesn't count as hit itself, so the siblings under its node are still tested
    /// (`Modifier.nested_scroll`).
    fn is_pointer_observer(&self) -> bool {
        false
    }
    /// Whether an update of the node by its element invalidates what its kinds say (a layout
    /// node is measured again, a draw node drawn again), as Compose's `shouldAutoInvalidate`.
    /// A node that invalidates itself, and only when its update changed something, says no.
    fn should_auto_invalidate(&self) -> bool {
        true
    }
    /// The node was attached, as `Modifier.Node.onAttach`: its layout node is in the tree and
    /// active, and every node of the layout nodes attached with it is marked attached already.
    /// Runs at the end of the composition pass that attached it, children's nodes before their
    /// parents', head to tail in a chain; again after each reuse. Its context reaches the tree:
    /// it may start work in `coroutine_scope()`, which detaching cancels.
    fn on_attach(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        crate::lifecycle::default_callback_ran();
    }
    /// The node is detached, as `Modifier.Node.onDetach`: its layout node left the tree, the node
    /// left its chain, or its content is deactivated or reused (after `on_reset`). Runs at the
    /// end of the pass, parents' nodes before their children's, tail to head in a chain; the
    /// node still attached, and its coroutine scope cancelled after. It may be attached again
    /// (reuse): what it holds outside itself is given back in `on_release`.
    fn on_detach(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        crate::lifecycle::default_callback_ran();
    }
    /// Its layout node is deactivated or reused for other content (a lazy list's item that
    /// left, reused by one that came), as Compose's `Modifier.Node.onReset`: forget what the old
    /// content's interactions left (a press, a hover, a gesture, a focus, an animation under
    /// way), then `on_detach` and `on_attach` follow. The node stays, and the new content's
    /// element updates it. Only an attached node is reset, as Compose's: a node reused from
    /// the kept pool was reset when it was deactivated (D1156).
    ///
    /// **The order differs from Compose's:** Compose resets a reused node before the new
    /// content's `update`; compose-rs applies changes as composition goes, where application
    /// code can't write state (D434), so the reset runs after the pass, *after* `update`. A
    /// reset therefore sees the new element's values (an interaction source swapped by
    /// `update` is the new one: release the old one there), and a node showing its value
    /// shows the new one from `on_attach`, which follows. It runs outside composition: it may
    /// write state. A reset changing what the node measures says so (`invalidate_measurement`):
    /// reuse alone measures nothing again.
    fn on_reset(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {}
    /// The density changed, as `DelegatableNode.onDensityChange` (the host's: compose-rs's
    /// density is its tree's).
    fn on_density_change(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        crate::lifecycle::default_callback_ran();
    }
    /// Its layout node's layout direction changed, as `onLayoutDirectionChange`.
    fn on_layout_direction_change(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        crate::lifecycle::default_callback_ran();
    }
    /// The node is dropped (removed from its chain, or its layout node released), after its
    /// `on_detach`: give back what it holds outside itself (a paragraph to the text measurer).
    /// compose-rs's: Compose's nodes are collected. Kept apart from `on_detach` so a node
    /// detached for reuse keeps its caches (D1153).
    fn on_release(&mut self, _context: &mut ReleaseContext<'_>) {}
    /// Asked to show `rect` (in this node's coordinates; `viewport` is its size), as Compose's
    /// `BringIntoViewResponder`: a scrolling node scrolls so it is in view, and returns where it
    /// will then be, for the scrolling ancestors. `None`: not a scrolling node.
    fn bring_into_view(
        &mut self,
        _rect: crate::unit::Rect,
        _viewport: crate::unit::IntSize,
    ) -> Option<crate::unit::Rect> {
        None
    }
    /// Whether it lays out content beyond its bounds for a focus search from inside it, as
    /// Compose's `BeyondBoundsLayout` (a lazy list's).
    fn is_beyond_bounds_layout(&self) -> bool {
        false
    }
    /// A focus search from inside it going `direction` found nothing more inside: lays out more
    /// content that way, beyond its bounds (a lazy list: one more item, composed and placed at
    /// the next layout), until `end_beyond_bounds`, as `BeyondBoundsLayout.layout`. Returns
    /// whether there's more that way: the search is made again once it's laid out (D477).
    fn lay_out_beyond_bounds(
        &mut self,
        _direction: crate::focus_manager::FocusDirection,
        _layout_direction: crate::alignment::LayoutDirection,
    ) -> bool {
        false
    }
    /// The focus search is over: what was laid out beyond the bounds for it goes.
    fn end_beyond_bounds(&mut self) {}
}

impl dyn ModifierNode {
    pub fn downcast_mut<T: ModifierNode>(&mut self) -> Option<&mut T> {
        (self as &mut dyn Any).downcast_mut::<T>()
    }

    pub fn downcast_ref<T: ModifierNode>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}

/// What a node reaches when released (`ModifierNode::on_release`).
pub struct ReleaseContext<'a> {
    pub text_measurer: &'a mut dyn crate::text::TextMeasurer,
}

/// A node that changes how its content is measured and placed, as `LayoutModifierNode`.
///
/// Its intrinsic functions default to Compose's (`NodeMeasuringIntrinsics`): `measure` run in
/// an intrinsic measurement, where measuring `content` returns its intrinsic on the axis asked
/// and placing does nothing (see [`MeasurePolicy`](crate::MeasurePolicy)).
pub trait LayoutModifierNode {
    /// Measures `content` (the rest of the chain and the layout node's own policy) and places
    /// it, returning this node's size.
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult;

    /// As `LayoutModifierNode.minIntrinsicWidth`: this layer's smallest width at `height`.
    fn min_intrinsic_width(
        &mut self,
        scope: &mut crate::layout::IntrinsicMeasureScope<'_>,
        content: crate::layout::IntrinsicMeasurable,
        height: i32,
    ) -> i32 {
        crate::layout::Intrinsic::MinWidth
            .measured(scope, height, |scope, constraints| self.measure(scope, content, constraints))
    }

    /// As `LayoutModifierNode.minIntrinsicHeight`: this layer's smallest height at `width`.
    fn min_intrinsic_height(
        &mut self,
        scope: &mut crate::layout::IntrinsicMeasureScope<'_>,
        content: crate::layout::IntrinsicMeasurable,
        width: i32,
    ) -> i32 {
        crate::layout::Intrinsic::MinHeight
            .measured(scope, width, |scope, constraints| self.measure(scope, content, constraints))
    }

    /// As `LayoutModifierNode.maxIntrinsicWidth`: the width beyond which this layer gets no
    /// lower, at `height`.
    fn max_intrinsic_width(
        &mut self,
        scope: &mut crate::layout::IntrinsicMeasureScope<'_>,
        content: crate::layout::IntrinsicMeasurable,
        height: i32,
    ) -> i32 {
        crate::layout::Intrinsic::MaxWidth
            .measured(scope, height, |scope, constraints| self.measure(scope, content, constraints))
    }

    /// As `LayoutModifierNode.maxIntrinsicHeight`: the height beyond which this layer gets no
    /// narrower, at `width`.
    fn max_intrinsic_height(
        &mut self,
        scope: &mut crate::layout::IntrinsicMeasureScope<'_>,
        content: crate::layout::IntrinsicMeasurable,
        width: i32,
    ) -> i32 {
        crate::layout::Intrinsic::MaxHeight
            .measured(scope, width, |scope, constraints| self.measure(scope, content, constraints))
    }
}

/// A node that draws, as `DrawModifierNode`: `draw` before the content (the rest of the chain
/// and the children) and `draw_after` after it, which is `ContentDrawScope.drawContent()` split
/// in two.
pub trait DrawModifierNode {
    fn draw(&mut self, _scope: &mut DrawScope<'_>) {}
    fn draw_after(&mut self, _scope: &mut DrawScope<'_>) {}
    /// Whether it draws the content itself, with `draw_with_content` (instead of `draw` and
    /// `draw_after` around it).
    fn draws_content(&self) -> bool {
        false
    }
    /// Draws, the content where it calls `scope.draw_content()`, as `ContentDrawScope`.
    fn draw_with_content(&mut self, _scope: &mut crate::draw_modifiers::ContentDrawScope<'_, '_>) {}
}

/// A node that receives pointer events hitting its bounds, as `PointerInputModifierNode`.
pub trait PointerInputModifierNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    );
    /// The gesture ended without its events (the node left, or another took the pointer).
    fn on_cancel_pointer_input(&mut self) {}
    /// The mouse pointer's look over it, as `Modifier.pointerHoverIcon` (the innermost on the
    /// hovered path wins).
    fn hover_icon(&self) -> Option<crate::input::PointerIcon> {
        None
    }
}

/// A node that adds semantics, as `SemanticsModifierNode`.
pub trait SemanticsModifierNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration);
    /// Performs `action` if this node offers it (as its semantics say), returning whether it
    /// did.
    fn perform_action(
        &mut self,
        _action: crate::semantics::SemanticsAction,
        _context: &mut EventContext<'_>,
    ) -> bool {
        false
    }
}

/// A focus target, as `FocusTargetModifierNode`.
pub trait FocusTargetModifierNode {
    fn can_focus(&self) -> bool {
        true
    }
    fn on_focus_changed(&mut self, _focused: bool, _context: &mut EventContext<'_>) {}
    /// Focus came from a tap at `position` (in this target's coordinates, possibly outside its
    /// bounds), on a decoration around it.
    fn on_focus_tap(&mut self, _position: crate::unit::Offset, _context: &mut EventContext<'_>) {}
}

/// A node that receives key and text events while it or a descendant has focus, as
/// `KeyInputModifierNode`. Returning true consumes the event.
pub trait KeyInputModifierNode {
    fn on_key_event(&mut self, _event: &KeyEvent, _context: &mut EventContext<'_>) -> bool {
        false
    }
    /// Committed text from the keyboard or an input method.
    fn on_text_input(&mut self, _text: &str, _context: &mut EventContext<'_>) -> bool {
        false
    }
    /// The text an input method is composing (pinyin, kana, a dead key), shown until it is
    /// committed or cancelled (empty), with the cursor's byte range in it, if any.
    fn on_preedit(
        &mut self,
        _text: &str,
        _cursor: Option<(usize, usize)>,
        _context: &mut EventContext<'_>,
    ) -> bool {
        false
    }
    /// Where text input goes, in the node's coordinates: an input method places its candidate
    /// window there. `None` if the node takes no text.
    fn text_input_area(&self) -> Option<crate::unit::Rect> {
        None
    }
    /// What the node asks of an on-screen keyboard while it takes text (`text_input_area` is
    /// `Some`), as Compose's `ImeOptions`. `None` for the defaults.
    fn ime_options(&self) -> Option<crate::text_input::ImeOptions> {
        None
    }
    /// The input method's action key (Android's editor action) was pressed, as Compose's
    /// `onImeActionPerformed`. Returning true consumes it.
    fn on_ime_action(
        &mut self,
        _action: crate::text_input::ImeAction,
        _context: &mut EventContext<'_>,
    ) -> bool {
        false
    }
}
