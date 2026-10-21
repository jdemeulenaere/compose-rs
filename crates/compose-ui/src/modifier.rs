//! `Modifier`: an ordered chain of elements, each backed by a `ModifierNode` on its layout node
//! (docs/06-ui.md, "Modifiers"). Built-in elements are enum variants kept inline, so a chain of
//! up to four allocates nothing; user-defined modifiers are `Custom` elements.

use std::any::Any;
use std::fmt;
use std::rc::Rc;

use compose_runtime::{Callback, ComposeArg, IntoCallback};
use smallvec::SmallVec;

use crate::alignment::{Alignment, HorizontalAlignment, VerticalAlignment};
use crate::constraints::Constraints;
use crate::draw::DrawScope;
use crate::graphics::{Color, Shape};
use crate::input::{EventContext, KeyEvent, PointerEvent, PointerEventPass};
use crate::layout::{Measurable, MeasureResult, MeasureScope};
use crate::semantics::{Role, SemanticsConfiguration};
use crate::text::SharedString;
use crate::unit::{Dp, IntSize};

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
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// This chain followed by `element`.
    pub fn then_element(mut self, element: Element) -> Modifier {
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

    pub fn padding(self, all: Dp) -> Modifier {
        self.padding_values(PaddingValues::all(all))
    }

    pub fn padding_horizontal_vertical(self, horizontal: Dp, vertical: Dp) -> Modifier {
        self.padding_values(PaddingValues::symmetric(horizontal, vertical))
    }

    pub fn padding_sides(self, start: Dp, top: Dp, end: Dp, bottom: Dp) -> Modifier {
        self.padding_values(PaddingValues::new(start, top, end, bottom))
    }

    pub fn padding_values(self, padding: PaddingValues) -> Modifier {
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

    pub fn size(self, size: Dp) -> Modifier {
        self.size_in(SizeSpec::exact(size, size, false))
    }

    pub fn size_wh(self, width: Dp, height: Dp) -> Modifier {
        self.size_in(SizeSpec::exact(width, height, false))
    }

    pub fn width(self, width: Dp) -> Modifier {
        self.size_in(SizeSpec::exact(width, Dp::UNSPECIFIED, false))
    }

    pub fn height(self, height: Dp) -> Modifier {
        self.size_in(SizeSpec::exact(Dp::UNSPECIFIED, height, false))
    }

    /// Exact sizes, overriding the incoming constraints, as `Modifier.requiredSize`.
    pub fn required_size(self, width: Dp, height: Dp) -> Modifier {
        self.size_in(SizeSpec::exact(width, height, true))
    }

    /// Bounds on the size, as `Modifier.sizeIn` (`Dp::UNSPECIFIED` for no bound).
    pub fn size_in(self, spec: SizeSpec) -> Modifier {
        self.then_element(Element::Size(spec))
    }

    pub fn fill_max_width(self) -> Modifier {
        self.fill_max_width_fraction(1.0)
    }

    pub fn fill_max_height(self) -> Modifier {
        self.fill_max_height_fraction(1.0)
    }

    pub fn fill_max_size(self) -> Modifier {
        self.fill_max_size_fraction(1.0)
    }

    pub fn fill_max_width_fraction(self, fraction: f32) -> Modifier {
        self.then_element(Element::Fill(Fill { horizontal: Some(fraction), vertical: None }))
    }

    pub fn fill_max_height_fraction(self, fraction: f32) -> Modifier {
        self.then_element(Element::Fill(Fill { horizontal: None, vertical: Some(fraction) }))
    }

    pub fn fill_max_size_fraction(self, fraction: f32) -> Modifier {
        self.then_element(Element::Fill(Fill {
            horizontal: Some(fraction),
            vertical: Some(fraction),
        }))
    }

    /// A share of the remaining space in a `Row` or `Column`, as `RowScope.weight`.
    pub fn weight(self, _scope: impl WeightScope, weight: f32) -> Modifier {
        self.weight_filled(_scope, weight, true)
    }

    /// `weight` with `fill`: whether the child must take its whole share.
    pub fn weight_filled(self, _scope: impl WeightScope, weight: f32, fill: bool) -> Modifier {
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

    pub fn background(self, color: Color) -> Modifier {
        self.background_shape(color, Shape::Rectangle)
    }

    pub fn background_shape(self, color: Color, shape: Shape) -> Modifier {
        self.then_element(Element::Background(color, shape))
    }

    pub fn border(self, width: Dp, color: Color, shape: Shape) -> Modifier {
        self.then_element(Element::Border(width, color, shape))
    }

    /// Clips the content to `shape`, as `Modifier.clip`.
    pub fn clip(self, shape: Shape) -> Modifier {
        self.then_element(Element::Clip(shape))
    }

    // ---- Input ------------------------------------------------------------------------------

    /// Calls `on_click` on a press released inside, and on Enter or Space when focused, as
    /// `Modifier.clickable`.
    pub fn clickable(self, on_click: impl IntoCallback<dyn Fn()>) -> Modifier {
        self.clickable_with(true, None, on_click)
    }

    pub fn clickable_with(
        self,
        enabled: bool,
        role: Option<Role>,
        on_click: impl IntoCallback<dyn Fn()>,
    ) -> Modifier {
        self.clickable_indicated(enabled, role, Color::UNSPECIFIED, on_click)
    }

    /// `clickable_with` drawing a state layer of `indication` while pressed or focused, as
    /// Compose's `clickable(indication = …)` with Material's ripple (without its animation).
    pub fn clickable_indicated(
        self,
        enabled: bool,
        role: Option<Role>,
        indication: Color,
        on_click: impl IntoCallback<dyn Fn()>,
    ) -> Modifier {
        self.then_element(Element::Clickable(Clickable {
            enabled,
            role,
            indication,
            on_click: on_click.into_callback(),
        }))
    }

    /// A toggle: a click calls `on_value_change(!value)`, as `Modifier.toggleable`.
    pub fn toggleable(
        self,
        value: bool,
        enabled: bool,
        role: Option<Role>,
        on_value_change: impl IntoCallback<dyn Fn(bool)>,
    ) -> Modifier {
        self.toggleable_indicated(value, enabled, role, Color::UNSPECIFIED, on_value_change)
    }

    /// `toggleable` drawing a state layer of `indication`, as `clickable_indicated`.
    pub fn toggleable_indicated(
        self,
        value: bool,
        enabled: bool,
        role: Option<Role>,
        indication: Color,
        on_value_change: impl IntoCallback<dyn Fn(bool)>,
    ) -> Modifier {
        self.then_element(Element::Toggleable(Toggleable {
            value,
            enabled,
            role,
            indication,
            on_value_change: on_value_change.into_callback(),
        }))
    }

    /// Makes the element a focus target, as `Modifier.focusable`.
    pub fn focusable(self, enabled: bool) -> Modifier {
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
    pub fn semantics(
        self,
        merge_descendants: bool,
        properties: impl FnOnce(&mut SemanticsConfiguration),
    ) -> Modifier {
        let mut config = SemanticsConfiguration::default();
        properties(&mut config);
        self.then_element(Element::Semantics(merge_descendants, Rc::new(config)))
    }
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

/// Padding in `Dp`, as `PaddingValues`.
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
pub struct Fill {
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

/// `clickable`'s data.
#[derive(Clone)]
pub struct Clickable {
    pub enabled: bool,
    pub role: Option<Role>,
    /// The state layer's color, drawn at 10% while pressed or focused (Material's indication);
    /// unspecified for none.
    pub indication: Color,
    pub on_click: Callback<dyn Fn()>,
}

/// `toggleable`'s data.
#[derive(Clone)]
pub struct Toggleable {
    pub value: bool,
    pub enabled: bool,
    pub role: Option<Role>,
    /// As `Clickable::indication`.
    pub indication: Color,
    pub on_value_change: Callback<dyn Fn(bool)>,
}

/// One modifier element: plain data, compared on recomposition to update its node in place.
#[derive(Clone)]
pub enum Element {
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
    Custom(Rc<dyn ModifierNodeElement>),
}

impl Element {
    /// Whether `other` is the same kind of element, so its node can be updated in place.
    pub fn same_kind(&self, other: &Element) -> bool {
        match (self, other) {
            (Element::Custom(a), Element::Custom(b)) => (**a).type_id() == (**b).type_id(),
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }

    /// Whether `other` is equal: then its node needs no update.
    pub fn same(&self, other: &Element) -> bool {
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
            (Custom(a), Custom(b)) => Rc::ptr_eq(a, b) || ModifierNodeElement::same(&**a, &**b),
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
            Custom(element) => {
                write!(formatter, "{}", crate::tree::short_type_name(element.name()))
            }
        }
    }
}

// ---- Modifier.Node ---------------------------------------------------------------------------

/// A user-defined modifier element, as Compose's `ModifierNodeElement`: it creates its node,
/// and updates it in place when a new element of the same type replaces it.
pub trait ModifierNodeElement: Any {
    fn create(&self) -> Box<dyn ModifierNode>;
    /// Updates `node`, which this element's type created.
    fn update(&self, node: &mut dyn ModifierNode);
    /// Whether `other` (of the same type, as `as_any` downcasts) is equal, so the node needs no
    /// update.
    fn same(&self, other: &dyn ModifierNodeElement) -> bool;
    /// A name for dumps.
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

impl dyn ModifierNodeElement {
    /// `self` as `T`, for `same`.
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
    /// The shape this node clips its content to, for hit testing: what's outside isn't hit
    /// (`clip`, a scroll's viewport), as a Compose graphics layer clips.
    fn hit_clip(&self) -> Option<crate::graphics::Shape> {
        None
    }
    /// Called when the node leaves the tree, before it's dropped: release resources here.
    fn on_detach(&mut self, _context: &mut DetachContext<'_>) {}
}

impl dyn ModifierNode {
    pub fn downcast_mut<T: ModifierNode>(&mut self) -> Option<&mut T> {
        (self as &mut dyn Any).downcast_mut::<T>()
    }

    pub fn downcast_ref<T: ModifierNode>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}

/// What a node can reach when detached.
pub struct DetachContext<'a> {
    pub text_measurer: &'a mut dyn crate::text::TextMeasurer,
}

/// A node that changes how its content is measured and placed, as `LayoutModifierNode`.
pub trait LayoutModifierNode {
    /// Measures `content` (the rest of the chain and the layout node's own policy) and places
    /// it, returning this node's size.
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult;
}

/// A node that draws, as `DrawModifierNode`: `draw` before the content (the rest of the chain
/// and the children) and `draw_after` after it, which is `ContentDrawScope.drawContent()` split
/// in two.
pub trait DrawModifierNode {
    fn draw(&mut self, _scope: &mut DrawScope<'_>) {}
    fn draw_after(&mut self, _scope: &mut DrawScope<'_>) {}
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
}

/// A node that adds semantics, as `SemanticsModifierNode`.
pub trait SemanticsModifierNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration);
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
}
