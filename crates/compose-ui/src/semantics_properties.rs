//! Semantics properties as Compose's: typed keys (`SemanticsPropertyKey`, declared as statics
//! and compared by address) with their merge policies, the values they hold (inline for the
//! common types, `ValueSlot`), the actions (`AccessibilityAction`, `CustomAccessibilityAction`),
//! the sets (`SemanticsProperties`, `SemanticsActions`), and the configuration a node's
//! modifiers fill, with Compose's `SemanticsPropertyReceiver` setters (D1250).

use std::any::Any;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;

use smallvec::SmallVec;

use crate::annotated::AnnotatedString;
use crate::graphics::{Color, Shape};
use crate::semantics::{ProgressBarRangeInfo, Role, ScrollAxisRange, ToggleableState};
use crate::text::{SharedString, TextLayoutResult};
use crate::text_input::ImeAction;
use crate::unit::Offset;

// ---- Values ------------------------------------------------------------------------------------

/// A list of strings, as Compose's `List<String>` (`ContentDescription`) and
/// `List<AnnotatedString>` (`Text`, plain strings here): inline for one.
pub type StringList = SmallVec<[SharedString; 1]>;

/// The number of rows and columns of a collection (a list, a grid, a group of radio buttons),
/// as Compose's `CollectionInfo` (-1: unknown).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CollectionInfo {
    pub row_count: i32,
    pub column_count: i32,
}

/// Where an item is in its collection, as Compose's `CollectionItemInfo`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CollectionItemInfo {
    pub row_index: i32,
    pub row_span: i32,
    pub column_index: i32,
    pub column_span: i32,
}

/// How eagerly accessibility services announce a node's changes, as Compose's `LiveRegionMode`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LiveRegionMode {
    /// After what's being said.
    Polite,
    /// At once, interrupting.
    Assertive,
}

/// The state of an input method's suggestion in a text field, as Compose's
/// `InputTextSuggestionState`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct InputTextSuggestionState {
    pub is_committed_by_input_method_editor: bool,
    pub is_transliteration_suggestion_selected: bool,
}

/// A color computed when read, as Compose's `ColorProducer` (`BackgroundColor`). Equal when
/// it's the same function.
#[derive(Clone)]
pub struct ColorProducer(pub Rc<dyn Fn() -> Color>);

impl PartialEq for ColorProducer {
    fn eq(&self, other: &Self) -> bool {
        same_rc(&self.0, &other.0)
    }
}

impl fmt::Debug for ColorProducer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", (self.0)())
    }
}

/// A lazy layout's item index for a key, as Compose's `IndexForKey`'s `(Any) -> Int` (-1: no
/// such key).
#[derive(Clone)]
pub struct IndexForKey(pub Rc<IndexForKeyFn>);

/// `IndexForKey`'s function.
pub type IndexForKeyFn = dyn Fn(&dyn Any) -> i32;

impl PartialEq for IndexForKey {
    fn eq(&self, other: &Self) -> bool {
        same_rc(&self.0, &other.0)
    }
}

impl fmt::Debug for IndexForKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IndexForKey")
    }
}

/// A scroll by an offset that resolves to the offset consumed, as Compose's `ScrollByOffset`'s
/// suspending function.
pub type ScrollByOffsetFn = dyn Fn(Offset) -> Pin<Box<dyn Future<Output = Offset>>>;

/// `ScrollByOffset`'s value.
#[derive(Clone)]
pub struct ScrollByOffset(pub Rc<ScrollByOffsetFn>);

impl PartialEq for ScrollByOffset {
    fn eq(&self, other: &Self) -> bool {
        same_rc(&self.0, &other.0)
    }
}

impl fmt::Debug for ScrollByOffset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ScrollByOffset")
    }
}

/// Whether two `Rc`s point at the same value (their data, whatever their vtables).
fn same_rc<T: ?Sized>(a: &Rc<T>, b: &Rc<T>) -> bool {
    std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b))
}

// ---- Actions -----------------------------------------------------------------------------------

/// What `OnClick`, `RequestFocus`, `Expand`, the pages… do, as `() -> Boolean`: whether it was
/// handled.
pub type ActionFn = dyn Fn() -> bool;
/// `ScrollBy`'s `(x, y) -> Boolean`.
pub type ScrollByFn = dyn Fn(f32, f32) -> bool;
/// `ScrollToIndex`'s `(Int) -> Boolean`.
pub type ScrollToIndexFn = dyn Fn(i32) -> bool;
/// `SetProgress`'s `(Float) -> Boolean`.
pub type SetProgressFn = dyn Fn(f32) -> bool;
/// `SetSelection`'s `(start, end, relativeToOriginalText) -> Boolean`.
pub type SetSelectionFn = dyn Fn(i32, i32, bool) -> bool;
/// `SetText`'s, `SetTextSubstitution`'s and `InsertTextAtCursor`'s `(AnnotatedString) ->
/// Boolean`.
pub type TextActionFn = dyn Fn(AnnotatedString) -> bool;
/// `ShowTextSubstitution`'s `(Boolean) -> Boolean`.
pub type ShowTextSubstitutionFn = dyn Fn(bool) -> bool;
/// `GetTextLayoutResult`'s `(MutableList<TextLayoutResult>) -> Boolean`.
pub type GetTextLayoutResultFn = dyn Fn(&mut Vec<TextLayoutResult>) -> bool;
/// `GetScrollViewportLength`'s `(MutableList<Float>) -> Boolean`.
pub type GetScrollViewportLengthFn = dyn Fn(&mut Vec<f32>) -> bool;

/// An action a node offers, with the label accessibility services show for it, as Compose's
/// `AccessibilityAction`. Equal when the labels are and it's the same function.
pub struct AccessibilityAction<F: ?Sized> {
    pub label: Option<SharedString>,
    pub action: Option<Rc<F>>,
}

impl<F: ?Sized> AccessibilityAction<F> {
    pub fn new(label: Option<SharedString>, action: Rc<F>) -> AccessibilityAction<F> {
        AccessibilityAction { label, action: Some(action) }
    }
}

impl<F: ?Sized> Clone for AccessibilityAction<F> {
    fn clone(&self) -> Self {
        AccessibilityAction { label: self.label.clone(), action: self.action.clone() }
    }
}

impl<F: ?Sized> PartialEq for AccessibilityAction<F> {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label
            && match (&self.action, &other.action) {
                (Some(a), Some(b)) => same_rc(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

impl<F: ?Sized> fmt::Debug for AccessibilityAction<F> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "AccessibilityAction(label={:?}, action={})",
            self.label.as_deref(),
            if self.action.is_some() { "set" } else { "null" }
        )
    }
}

/// An action of the application's own, named by its label, as Compose's
/// `CustomAccessibilityAction`. Equal when the labels are and it's the same function.
#[derive(Clone)]
pub struct CustomAccessibilityAction {
    pub label: SharedString,
    pub action: Rc<ActionFn>,
}

impl CustomAccessibilityAction {
    pub fn new(
        label: impl Into<SharedString>,
        action: impl Fn() -> bool + 'static,
    ) -> CustomAccessibilityAction {
        CustomAccessibilityAction { label: label.into(), action: Rc::new(action) }
    }
}

impl PartialEq for CustomAccessibilityAction {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label && same_rc(&self.action, &other.action)
    }
}

impl fmt::Debug for CustomAccessibilityAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "CustomAccessibilityAction(label={:?})", self.label.as_str())
    }
}

// ---- Slots -------------------------------------------------------------------------------------

/// A value of an application's own key type, in `ValueSlot::Other`.
pub trait OtherValue {
    fn as_any(&self) -> &dyn Any;
    fn eq_other(&self, other: &dyn OtherValue) -> bool;
    fn fmt_other(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl<T: Clone + PartialEq + fmt::Debug + 'static> OtherValue for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn eq_other(&self, other: &dyn OtherValue) -> bool {
        other.as_any().downcast_ref::<T>().is_some_and(|other| other == self)
    }
    fn fmt_other(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

/// An `OtherValue` in a slot: one allocation, compared by value; a function (an application's
/// action, `ScrollByOffset`…) says so, to be printed by name.
#[derive(Clone)]
pub struct OtherSlot {
    value: Rc<dyn OtherValue>,
    is_action: bool,
}

impl OtherSlot {
    pub fn new(value: impl OtherValue + 'static) -> OtherSlot {
        OtherSlot { value: Rc::new(value), is_action: false }
    }

    /// A function's.
    pub fn action(value: impl OtherValue + 'static) -> OtherSlot {
        OtherSlot { value: Rc::new(value), is_action: true }
    }

    pub fn value(&self) -> &dyn OtherValue {
        &*self.value
    }
}

impl PartialEq for OtherSlot {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq_other(&*other.value)
    }
}

impl fmt::Debug for OtherSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt_other(formatter)
    }
}

/// A semantics value, held without boxing for the types Compose's keys use (D1250): an
/// application's own types go through `Other`, one `Rc` each.
#[doc(hidden)]
#[derive(Clone, PartialEq)]
pub enum ValueSlot {
    Unit,
    Bool(bool),
    Float(f32),
    Int(i32),
    String(SharedString),
    Strings(StringList),
    Role(Role),
    ToggleableState(ToggleableState),
    ProgressBarRangeInfo(ProgressBarRangeInfo),
    ScrollAxisRange(ScrollAxisRange),
    CollectionInfo(CollectionInfo),
    CollectionItemInfo(CollectionItemInfo),
    LiveRegion(LiveRegionMode),
    ImeAction(ImeAction),
    Shape(Shape),
    InputTextSuggestionState(InputTextSuggestionState),
    Action(AccessibilityAction<ActionFn>),
    ScrollByAction(AccessibilityAction<ScrollByFn>),
    ScrollToIndexAction(AccessibilityAction<ScrollToIndexFn>),
    SetProgressAction(AccessibilityAction<SetProgressFn>),
    SetSelectionAction(AccessibilityAction<SetSelectionFn>),
    TextAction(AccessibilityAction<TextActionFn>),
    ShowTextSubstitutionAction(AccessibilityAction<ShowTextSubstitutionFn>),
    GetTextLayoutResultAction(AccessibilityAction<GetTextLayoutResultFn>),
    GetScrollViewportLengthAction(AccessibilityAction<GetScrollViewportLengthFn>),
    CustomActions(Vec<CustomAccessibilityAction>),
    Other(OtherSlot),
}

impl ValueSlot {
    /// Whether it's an action (`AccessibilityAction`, a function): printed by name only.
    pub fn is_action(&self) -> bool {
        matches!(
            self,
            ValueSlot::Action(_)
                | ValueSlot::ScrollByAction(_)
                | ValueSlot::ScrollToIndexAction(_)
                | ValueSlot::SetProgressAction(_)
                | ValueSlot::SetSelectionAction(_)
                | ValueSlot::TextAction(_)
                | ValueSlot::ShowTextSubstitutionAction(_)
                | ValueSlot::GetTextLayoutResultAction(_)
                | ValueSlot::GetScrollViewportLengthAction(_)
        ) || matches!(self, ValueSlot::Other(other) if other.is_action)
    }

    /// This value set over `previous`: an action keeps `previous`'s label and function where it
    /// has none (Compose's `SemanticsConfiguration.set`); any other value replaces it.
    fn over(self, previous: &ValueSlot) -> ValueSlot {
        fn collapse<F: ?Sized>(
            new: AccessibilityAction<F>,
            old: &AccessibilityAction<F>,
        ) -> AccessibilityAction<F> {
            AccessibilityAction {
                label: new.label.or_else(|| old.label.clone()),
                action: new.action.or_else(|| old.action.clone()),
            }
        }
        use ValueSlot as V;
        match (self, previous) {
            (V::Action(new), V::Action(old)) => V::Action(collapse(new, old)),
            (V::ScrollByAction(new), V::ScrollByAction(old)) => {
                V::ScrollByAction(collapse(new, old))
            }
            (V::ScrollToIndexAction(new), V::ScrollToIndexAction(old)) => {
                V::ScrollToIndexAction(collapse(new, old))
            }
            (V::SetProgressAction(new), V::SetProgressAction(old)) => {
                V::SetProgressAction(collapse(new, old))
            }
            (V::SetSelectionAction(new), V::SetSelectionAction(old)) => {
                V::SetSelectionAction(collapse(new, old))
            }
            (V::TextAction(new), V::TextAction(old)) => V::TextAction(collapse(new, old)),
            (V::ShowTextSubstitutionAction(new), V::ShowTextSubstitutionAction(old)) => {
                V::ShowTextSubstitutionAction(collapse(new, old))
            }
            (V::GetTextLayoutResultAction(new), V::GetTextLayoutResultAction(old)) => {
                V::GetTextLayoutResultAction(collapse(new, old))
            }
            (V::GetScrollViewportLengthAction(new), V::GetScrollViewportLengthAction(old)) => {
                V::GetScrollViewportLengthAction(collapse(new, old))
            }
            (new, _) => new,
        }
    }

    /// Whether it's `Unit` (a flag: `Disabled`, `Heading`…).
    pub fn is_unit(&self) -> bool {
        matches!(self, ValueSlot::Unit)
    }

    /// The value as Compose prints it (`printToString`): strings bare, lists in brackets.
    pub fn display(&self) -> String {
        match self {
            ValueSlot::Unit => "kotlin.Unit".into(),
            ValueSlot::Bool(value) => value.to_string(),
            ValueSlot::Float(value) => value.to_string(),
            ValueSlot::Int(value) => value.to_string(),
            ValueSlot::String(value) => value.to_string(),
            ValueSlot::Strings(values) => {
                let list: Vec<&str> = values.iter().map(|value| value.as_str()).collect();
                format!("[{}]", list.join(", "))
            }
            other => format!("{other:?}"),
        }
    }
}

impl fmt::Debug for ValueSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValueSlot::Unit => formatter.write_str("()"),
            ValueSlot::Bool(value) => value.fmt(formatter),
            ValueSlot::Float(value) => value.fmt(formatter),
            ValueSlot::Int(value) => value.fmt(formatter),
            ValueSlot::String(value) => value.as_str().fmt(formatter),
            ValueSlot::Strings(values) => {
                formatter.debug_list().entries(values.iter().map(|value| value.as_str())).finish()
            }
            ValueSlot::Role(value) => value.fmt(formatter),
            ValueSlot::ToggleableState(value) => value.fmt(formatter),
            ValueSlot::ProgressBarRangeInfo(value) => value.fmt(formatter),
            ValueSlot::ScrollAxisRange(value) => value.fmt(formatter),
            ValueSlot::CollectionInfo(value) => value.fmt(formatter),
            ValueSlot::CollectionItemInfo(value) => value.fmt(formatter),
            ValueSlot::LiveRegion(value) => value.fmt(formatter),
            ValueSlot::ImeAction(value) => value.fmt(formatter),
            ValueSlot::Shape(value) => value.fmt(formatter),
            ValueSlot::InputTextSuggestionState(value) => value.fmt(formatter),
            ValueSlot::Action(value) => value.fmt(formatter),
            ValueSlot::ScrollByAction(value) => value.fmt(formatter),
            ValueSlot::ScrollToIndexAction(value) => value.fmt(formatter),
            ValueSlot::SetProgressAction(value) => value.fmt(formatter),
            ValueSlot::SetSelectionAction(value) => value.fmt(formatter),
            ValueSlot::TextAction(value) => value.fmt(formatter),
            ValueSlot::ShowTextSubstitutionAction(value) => value.fmt(formatter),
            ValueSlot::GetTextLayoutResultAction(value) => value.fmt(formatter),
            ValueSlot::GetScrollViewportLengthAction(value) => value.fmt(formatter),
            ValueSlot::CustomActions(value) => value.fmt(formatter),
            ValueSlot::Other(value) => value.fmt(formatter),
        }
    }
}

/// A type semantics can hold, as a key's `T`. Compose's value types are held inline; an
/// application's own type implements it with the defaults (`impl SemanticsValue for MyType {}`)
/// and is held through one `Rc` (D1250).
pub trait SemanticsValue: Clone + PartialEq + fmt::Debug + 'static {
    #[doc(hidden)]
    fn into_slot(self) -> ValueSlot {
        ValueSlot::Other(OtherSlot::new(self))
    }

    #[doc(hidden)]
    fn from_slot(slot: &ValueSlot) -> Option<&Self> {
        match slot {
            ValueSlot::Other(other) => other.value.as_any().downcast_ref::<Self>(),
            _ => None,
        }
    }
}

macro_rules! inline_value {
    ($($ty:ty => $variant:ident),* $(,)?) => {$(
        impl SemanticsValue for $ty {
            fn into_slot(self) -> ValueSlot {
                ValueSlot::$variant(self)
            }
            fn from_slot(slot: &ValueSlot) -> Option<&Self> {
                match slot {
                    ValueSlot::$variant(value) => Some(value),
                    _ => None,
                }
            }
        }
    )*};
}

inline_value! {
    bool => Bool,
    f32 => Float,
    i32 => Int,
    SharedString => String,
    StringList => Strings,
    Role => Role,
    ToggleableState => ToggleableState,
    ProgressBarRangeInfo => ProgressBarRangeInfo,
    ScrollAxisRange => ScrollAxisRange,
    CollectionInfo => CollectionInfo,
    CollectionItemInfo => CollectionItemInfo,
    LiveRegionMode => LiveRegion,
    ImeAction => ImeAction,
    Shape => Shape,
    InputTextSuggestionState => InputTextSuggestionState,
    Vec<CustomAccessibilityAction> => CustomActions,
}

impl SemanticsValue for () {
    fn into_slot(self) -> ValueSlot {
        ValueSlot::Unit
    }
    fn from_slot(slot: &ValueSlot) -> Option<&Self> {
        matches!(slot, ValueSlot::Unit).then_some(&())
    }
}

/// An action of any signature: Compose's signatures are held inline (their own variants), an
/// application's own (`AccessibilityAction<dyn Fn(String) -> bool>`) through `Other`. The
/// signature is found by its type, so applications declare action keys of their own without
/// implementing anything.
impl<F: ?Sized + 'static> SemanticsValue for AccessibilityAction<F> {
    fn into_slot(self) -> ValueSlot {
        let mut this = Some(self);
        macro_rules! inline {
            ($($signature:ty => $variant:ident),* $(,)?) => {$(
                if let Some(action) = (&mut this as &mut dyn Any)
                    .downcast_mut::<Option<AccessibilityAction<$signature>>>()
                {
                    return ValueSlot::$variant(action.take().expect("set once"));
                }
            )*};
        }
        inline! {
            ActionFn => Action,
            ScrollByFn => ScrollByAction,
            ScrollToIndexFn => ScrollToIndexAction,
            SetProgressFn => SetProgressAction,
            SetSelectionFn => SetSelectionAction,
            TextActionFn => TextAction,
            ShowTextSubstitutionFn => ShowTextSubstitutionAction,
            GetTextLayoutResultFn => GetTextLayoutResultAction,
            GetScrollViewportLengthFn => GetScrollViewportLengthAction,
        }
        ValueSlot::Other(OtherSlot::action(this.take().expect("set once")))
    }

    fn from_slot(slot: &ValueSlot) -> Option<&Self> {
        let value: &dyn Any = match slot {
            ValueSlot::Action(action) => action,
            ValueSlot::ScrollByAction(action) => action,
            ValueSlot::ScrollToIndexAction(action) => action,
            ValueSlot::SetProgressAction(action) => action,
            ValueSlot::SetSelectionAction(action) => action,
            ValueSlot::TextAction(action) => action,
            ValueSlot::ShowTextSubstitutionAction(action) => action,
            ValueSlot::GetTextLayoutResultAction(action) => action,
            ValueSlot::GetScrollViewportLengthAction(action) => action,
            ValueSlot::Other(other) => other.value.as_any(),
            _ => return None,
        };
        value.downcast_ref::<Self>()
    }
}

impl SemanticsValue for ColorProducer {}

impl SemanticsValue for IndexForKey {
    fn into_slot(self) -> ValueSlot {
        ValueSlot::Other(OtherSlot::action(self))
    }
}

impl SemanticsValue for ScrollByOffset {
    fn into_slot(self) -> ValueSlot {
        ValueSlot::Other(OtherSlot::action(self))
    }
}

// ---- Keys --------------------------------------------------------------------------------------

/// A semantics property, as Compose's `SemanticsPropertyKey<T>`: declared as a `static`, a key
/// is itself (compared by address, as Compose compares key objects), named, with the policy
/// merging a descendant's value into a merging ancestor's (D1250).
pub struct SemanticsPropertyKey<T> {
    name: &'static str,
    merge_policy: fn(Option<&T>, &T) -> Option<T>,
    important_for_accessibility: bool,
    /// Interior mutability keeps each key in memory of its own: two keys are never merged into
    /// one address, whatever their contents.
    _identity: AtomicBool,
}

/// Compose's default policy: the parent's value if it has one, else the child's.
fn parent_or_child<T: Clone>(parent: Option<&T>, child: &T) -> Option<T> {
    Some(parent.unwrap_or(child).clone())
}

/// The parent's value, never the child's: `Role`, `TestTag`, `TraversalIndex`…
fn parent_only<T: Clone>(parent: Option<&T>, _child: &T) -> Option<T> {
    parent.cloned()
}

/// The parent's values, then the child's: `Text`, `ContentDescription`.
fn concatenate(parent: Option<&StringList>, child: &StringList) -> Option<StringList> {
    let mut values = parent.cloned().unwrap_or_default();
    values.extend(child.iter().cloned());
    Some(values)
}

// The key's policy type takes the value as is.
#[allow(clippy::ptr_arg)]
fn concatenate_actions(
    parent: Option<&Vec<CustomAccessibilityAction>>,
    child: &Vec<CustomAccessibilityAction>,
) -> Option<Vec<CustomAccessibilityAction>> {
    let mut actions = parent.cloned().unwrap_or_default();
    actions.extend(child.iter().cloned());
    Some(actions)
}

/// An action's: the parent's label and action where it has them, else the child's.
fn merge_action<F: ?Sized>(
    parent: Option<&AccessibilityAction<F>>,
    child: &AccessibilityAction<F>,
) -> Option<AccessibilityAction<F>> {
    Some(AccessibilityAction {
        label: parent.and_then(|parent| parent.label.clone()).or_else(|| child.label.clone()),
        action: parent.and_then(|parent| parent.action.clone()).or_else(|| child.action.clone()),
    })
}

fn unmergeable_pane_title(_: Option<&SharedString>, _: &SharedString) -> Option<SharedString> {
    panic!(
        "merge function called on unmergeable property PaneTitle. A pane should not be a \
         descendant of a node that merges descendant semantics, such as a clickable."
    )
}

fn unmergeable_popup(_: Option<&()>, _: &()) -> Option<()> {
    panic!(
        "merge function called on unmergeable property IsPopup. A popup should not be a child \
         of a clickable/focusable node."
    )
}

fn unmergeable_dialog(_: Option<&()>, _: &()) -> Option<()> {
    panic!(
        "merge function called on unmergeable property IsDialog. A dialog should not be a child \
         of a clickable/focusable node."
    )
}

impl<T: SemanticsValue> SemanticsPropertyKey<T> {
    /// A key with Compose's default policy (the parent's value, else the child's), not marked
    /// important for accessibility, as `SemanticsPropertyKey(name)`.
    pub const fn new(name: &'static str) -> SemanticsPropertyKey<T> {
        SemanticsPropertyKey::with_merge_policy(name, parent_or_child::<T>)
    }

    /// A key with its own policy, as `SemanticsPropertyKey(name, mergePolicy)`: `None` leaves
    /// the merging node without the key.
    pub const fn with_merge_policy(
        name: &'static str,
        merge_policy: fn(Option<&T>, &T) -> Option<T>,
    ) -> SemanticsPropertyKey<T> {
        SemanticsPropertyKey {
            name,
            merge_policy,
            important_for_accessibility: false,
            _identity: AtomicBool::new(false),
        }
    }

    /// Marks the key important for accessibility (Compose's `AccessibilityKey`).
    pub const fn for_accessibility(mut self) -> SemanticsPropertyKey<T> {
        self.important_for_accessibility = true;
        self
    }

    /// The key's merge of a child's value into its parent's, as `merge`.
    pub fn merge(&self, parent: Option<&T>, child: &T) -> Option<T> {
        (self.merge_policy)(parent, child)
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn is_important_for_accessibility(&self) -> bool {
        self.important_for_accessibility
    }
}

impl<F: ?Sized + 'static> SemanticsPropertyKey<AccessibilityAction<F>>
where
    AccessibilityAction<F>: SemanticsValue,
{
    /// An action's key, as Compose's `ActionPropertyKey`: important for accessibility, merging
    /// label and action separately.
    pub const fn action(name: &'static str) -> SemanticsPropertyKey<AccessibilityAction<F>> {
        SemanticsPropertyKey::with_merge_policy(name, merge_action::<F>).for_accessibility()
    }
}

impl<T> fmt::Debug for SemanticsPropertyKey<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "AccessibilityKey: {}", self.name)
    }
}

/// A key whatever its type, as a configuration holds it.
pub trait AnySemanticsPropertyKey: 'static {
    fn name(&self) -> &'static str;
    fn is_important_for_accessibility(&self) -> bool;
    /// The key's policy over slots of its type.
    #[doc(hidden)]
    fn merge_slots(&self, parent: Option<&ValueSlot>, child: &ValueSlot) -> Option<ValueSlot>;
}

impl<T: SemanticsValue> AnySemanticsPropertyKey for SemanticsPropertyKey<T> {
    fn name(&self) -> &'static str {
        self.name
    }
    fn is_important_for_accessibility(&self) -> bool {
        self.important_for_accessibility
    }
    fn merge_slots(&self, parent: Option<&ValueSlot>, child: &ValueSlot) -> Option<ValueSlot> {
        let child = T::from_slot(child)?;
        let parent = parent.and_then(T::from_slot);
        self.merge(parent, child).map(T::into_slot)
    }
}

/// Whether `a` and `b` are the same key.
fn same_key(a: &dyn AnySemanticsPropertyKey, b: &dyn AnySemanticsPropertyKey) -> bool {
    std::ptr::addr_eq(a as *const dyn AnySemanticsPropertyKey, b as *const _)
}

// ---- The sets ----------------------------------------------------------------------------------

/// The properties semantics describe a node with, as Compose's `SemanticsProperties`, with its
/// policies. Left out: the deprecated keys (`IsContainer`, `InvisibleToUser`), the autofill keys
/// (`ContentType`, `ContentDataType`, `FillableData`: with ui-input's autofill), and the text
/// ranges (`TextSelectionRange`, `TextCompositionRange`: with ui-text's `TextRange`). `Text`,
/// `EditableText`, `InputText` and `TextSubstitution` hold plain strings, as compose-rs's text
/// semantics do.
#[allow(non_snake_case)]
pub mod SemanticsProperties {
    use super::*;

    pub static CONTENT_DESCRIPTION: SemanticsPropertyKey<StringList> =
        SemanticsPropertyKey::with_merge_policy("ContentDescription", concatenate)
            .for_accessibility();
    pub static STATE_DESCRIPTION: SemanticsPropertyKey<SharedString> =
        SemanticsPropertyKey::new("StateDescription").for_accessibility();
    pub static PROGRESS_BAR_RANGE_INFO: SemanticsPropertyKey<ProgressBarRangeInfo> =
        SemanticsPropertyKey::new("ProgressBarRangeInfo").for_accessibility();
    pub static PANE_TITLE: SemanticsPropertyKey<SharedString> =
        SemanticsPropertyKey::with_merge_policy("PaneTitle", unmergeable_pane_title)
            .for_accessibility();
    pub static SELECTABLE_GROUP: SemanticsPropertyKey<()> =
        SemanticsPropertyKey::new("SelectableGroup").for_accessibility();
    pub static COLLECTION_INFO: SemanticsPropertyKey<CollectionInfo> =
        SemanticsPropertyKey::new("CollectionInfo").for_accessibility();
    pub static COLLECTION_ITEM_INFO: SemanticsPropertyKey<CollectionItemInfo> =
        SemanticsPropertyKey::new("CollectionItemInfo").for_accessibility();
    pub static HEADING: SemanticsPropertyKey<()> =
        SemanticsPropertyKey::new("Heading").for_accessibility();
    pub static TEXT_ENTRY_KEY: SemanticsPropertyKey<()> =
        SemanticsPropertyKey::new("TextEntryKey").for_accessibility();
    pub static DISABLED: SemanticsPropertyKey<()> =
        SemanticsPropertyKey::new("Disabled").for_accessibility();
    pub static LIVE_REGION: SemanticsPropertyKey<LiveRegionMode> =
        SemanticsPropertyKey::new("LiveRegion").for_accessibility();
    pub static FOCUSED: SemanticsPropertyKey<bool> =
        SemanticsPropertyKey::new("Focused").for_accessibility();
    pub static IS_TRAVERSAL_GROUP: SemanticsPropertyKey<bool> =
        SemanticsPropertyKey::new("IsTraversalGroup");
    pub static IS_SENSITIVE_DATA: SemanticsPropertyKey<bool> =
        SemanticsPropertyKey::new("IsSensitiveData");
    pub static HIDE_FROM_ACCESSIBILITY: SemanticsPropertyKey<()> =
        SemanticsPropertyKey::with_merge_policy("HideFromAccessibility", parent_only);
    pub static TRAVERSAL_INDEX: SemanticsPropertyKey<f32> =
        SemanticsPropertyKey::with_merge_policy("TraversalIndex", parent_only);
    pub static HINT_TEXT: SemanticsPropertyKey<SharedString> =
        SemanticsPropertyKey::new("HintText").for_accessibility();
    pub static HORIZONTAL_SCROLL_AXIS_RANGE: SemanticsPropertyKey<ScrollAxisRange> =
        SemanticsPropertyKey::new("HorizontalScrollAxisRange").for_accessibility();
    pub static VERTICAL_SCROLL_AXIS_RANGE: SemanticsPropertyKey<ScrollAxisRange> =
        SemanticsPropertyKey::new("VerticalScrollAxisRange").for_accessibility();
    pub static IS_POPUP: SemanticsPropertyKey<()> =
        SemanticsPropertyKey::with_merge_policy("IsPopup", unmergeable_popup).for_accessibility();
    pub static IS_DIALOG: SemanticsPropertyKey<()> =
        SemanticsPropertyKey::with_merge_policy("IsDialog", unmergeable_dialog).for_accessibility();
    pub static ROLE: SemanticsPropertyKey<Role> =
        SemanticsPropertyKey::with_merge_policy("Role", parent_only).for_accessibility();
    pub static TEST_TAG: SemanticsPropertyKey<SharedString> =
        SemanticsPropertyKey::with_merge_policy("TestTag", parent_only);
    pub static LINK_TEST_MARKER: SemanticsPropertyKey<()> =
        SemanticsPropertyKey::with_merge_policy("LinkTestMarker", parent_only);
    pub static TEXT: SemanticsPropertyKey<StringList> =
        SemanticsPropertyKey::with_merge_policy("Text", concatenate).for_accessibility();
    pub static TEXT_SUBSTITUTION: SemanticsPropertyKey<SharedString> =
        SemanticsPropertyKey::new("TextSubstitution");
    pub static IS_SHOWING_TEXT_SUBSTITUTION: SemanticsPropertyKey<bool> =
        SemanticsPropertyKey::new("IsShowingTextSubstitution");
    pub static INPUT_TEXT: SemanticsPropertyKey<SharedString> =
        SemanticsPropertyKey::new("InputText").for_accessibility();
    pub static EDITABLE_TEXT: SemanticsPropertyKey<SharedString> =
        SemanticsPropertyKey::new("EditableText").for_accessibility();
    pub static IME_ACTION: SemanticsPropertyKey<ImeAction> =
        SemanticsPropertyKey::new("ImeAction").for_accessibility();
    pub static SELECTED: SemanticsPropertyKey<bool> =
        SemanticsPropertyKey::new("Selected").for_accessibility();
    pub static TOGGLEABLE_STATE: SemanticsPropertyKey<ToggleableState> =
        SemanticsPropertyKey::new("ToggleableState").for_accessibility();
    pub static INPUT_TEXT_SUGGESTION_STATE: SemanticsPropertyKey<InputTextSuggestionState> =
        SemanticsPropertyKey::new("InputTextSuggestionState").for_accessibility();
    pub static PASSWORD: SemanticsPropertyKey<()> =
        SemanticsPropertyKey::new("Password").for_accessibility();
    pub static IS_PASSWORD_OBFUSCATED: SemanticsPropertyKey<bool> =
        SemanticsPropertyKey::new("IsPasswordObfuscated").for_accessibility();
    pub static ERROR: SemanticsPropertyKey<SharedString> =
        SemanticsPropertyKey::new("Error").for_accessibility();
    pub static INDEX_FOR_KEY: SemanticsPropertyKey<IndexForKey> =
        SemanticsPropertyKey::new("IndexForKey");
    pub static IS_EDITABLE: SemanticsPropertyKey<bool> = SemanticsPropertyKey::new("IsEditable");
    pub static MAX_TEXT_LENGTH: SemanticsPropertyKey<i32> =
        SemanticsPropertyKey::new("MaxTextLength");
    pub static BACKGROUND_COLOR: SemanticsPropertyKey<ColorProducer> =
        SemanticsPropertyKey::with_merge_policy("BackgroundColor", parent_only);
    pub static SHAPE: SemanticsPropertyKey<Shape> =
        SemanticsPropertyKey::with_merge_policy("Shape", parent_only);
}

/// The actions semantics offer, as Compose's `SemanticsActions`. Left out: the deprecated
/// `OnAutofillText` and `PerformImeAction` (`OnImeAction` is named `PerformImeAction`, as
/// Compose's), and `OnFillData` (with ui-input's autofill).
#[allow(non_snake_case)]
pub mod SemanticsActions {
    use super::*;

    pub static GET_TEXT_LAYOUT_RESULT: SemanticsPropertyKey<
        AccessibilityAction<GetTextLayoutResultFn>,
    > = SemanticsPropertyKey::action("GetTextLayoutResult");
    pub static ON_CLICK: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("OnClick");
    pub static ON_LONG_CLICK: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("OnLongClick");
    pub static SCROLL_BY: SemanticsPropertyKey<AccessibilityAction<ScrollByFn>> =
        SemanticsPropertyKey::action("ScrollBy");
    pub static SCROLL_BY_OFFSET: SemanticsPropertyKey<ScrollByOffset> =
        SemanticsPropertyKey::new("ScrollByOffset");
    pub static SCROLL_TO_INDEX: SemanticsPropertyKey<AccessibilityAction<ScrollToIndexFn>> =
        SemanticsPropertyKey::action("ScrollToIndex");
    pub static SET_PROGRESS: SemanticsPropertyKey<AccessibilityAction<SetProgressFn>> =
        SemanticsPropertyKey::action("SetProgress");
    pub static SET_SELECTION: SemanticsPropertyKey<AccessibilityAction<SetSelectionFn>> =
        SemanticsPropertyKey::action("SetSelection");
    pub static SET_TEXT: SemanticsPropertyKey<AccessibilityAction<TextActionFn>> =
        SemanticsPropertyKey::action("SetText");
    pub static SET_TEXT_SUBSTITUTION: SemanticsPropertyKey<AccessibilityAction<TextActionFn>> =
        SemanticsPropertyKey::action("SetTextSubstitution");
    pub static SHOW_TEXT_SUBSTITUTION: SemanticsPropertyKey<
        AccessibilityAction<ShowTextSubstitutionFn>,
    > = SemanticsPropertyKey::action("ShowTextSubstitution");
    pub static CLEAR_TEXT_SUBSTITUTION: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("ClearTextSubstitution");
    pub static INSERT_TEXT_AT_CURSOR: SemanticsPropertyKey<AccessibilityAction<TextActionFn>> =
        SemanticsPropertyKey::action("InsertTextAtCursor");
    pub static ON_IME_ACTION: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("PerformImeAction");
    pub static COPY_TEXT: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("CopyText");
    pub static CUT_TEXT: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("CutText");
    pub static PASTE_TEXT: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("PasteText");
    pub static EXPAND: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("Expand");
    pub static COLLAPSE: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("Collapse");
    pub static DISMISS: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("Dismiss");
    pub static REQUEST_FOCUS: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("RequestFocus");
    pub static CUSTOM_ACTIONS: SemanticsPropertyKey<Vec<CustomAccessibilityAction>> =
        SemanticsPropertyKey::with_merge_policy("CustomActions", concatenate_actions)
            .for_accessibility();
    pub static PAGE_UP: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("PageUp");
    pub static PAGE_LEFT: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("PageLeft");
    pub static PAGE_DOWN: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("PageDown");
    pub static PAGE_RIGHT: SemanticsPropertyKey<AccessibilityAction<ActionFn>> =
        SemanticsPropertyKey::action("PageRight");
    pub static GET_SCROLL_VIEWPORT_LENGTH: SemanticsPropertyKey<
        AccessibilityAction<GetScrollViewportLengthFn>,
    > = SemanticsPropertyKey::action("GetScrollViewportLength");
}

/// compose-rs's own: the properties belong to the nearest ancestor with semantics, in both
/// trees, as if its own modifiers set them (a text field's inner text, which in Compose is one
/// semantics node with its decoration box).
pub static PART_OF_PARENT: SemanticsPropertyKey<()> =
    SemanticsPropertyKey::with_merge_policy("PartOfParent", parent_only);

// ---- The configuration -------------------------------------------------------------------------

/// A key and its value.
#[derive(Clone)]
struct Entry {
    key: &'static dyn AnySemanticsPropertyKey,
    value: ValueSlot,
}

/// The semantics properties of a node, as Compose's `SemanticsConfiguration`: a small map of
/// keys to values (inline up to four), whether the node merges its descendants' and whether it
/// clears them. Its setters are Compose's `SemanticsPropertyReceiver`'s.
#[derive(Clone, Default)]
pub struct SemanticsConfiguration {
    entries: SmallVec<[Entry; 4]>,
    /// Merge descendants' semantics into this node, as a button merges its label
    /// (`isMergingSemanticsOfDescendants`).
    pub is_merging_semantics_of_descendants: bool,
    /// Its descendants have no semantics, as `clearAndSetSemantics` (`isClearingSemantics`).
    pub is_clearing_semantics: bool,
}

impl PartialEq for SemanticsConfiguration {
    fn eq(&self, other: &Self) -> bool {
        self.is_merging_semantics_of_descendants == other.is_merging_semantics_of_descendants
            && self.is_clearing_semantics == other.is_clearing_semantics
            && self.entries.len() == other.entries.len()
            && self
                .entries
                .iter()
                .all(|entry| other.slot(entry.key).is_some_and(|value| *value == entry.value))
    }
}

impl SemanticsConfiguration {
    fn position(&self, key: &dyn AnySemanticsPropertyKey) -> Option<usize> {
        self.entries.iter().position(|entry| same_key(entry.key, key))
    }

    fn slot(&self, key: &dyn AnySemanticsPropertyKey) -> Option<&ValueSlot> {
        self.position(key).map(|index| &self.entries[index].value)
    }

    /// Sets `key`'s slot, as Compose's `set`: an action over an action keeps the earlier one's
    /// label and function where the new one has none.
    fn set_slot(&mut self, key: &'static dyn AnySemanticsPropertyKey, value: ValueSlot) {
        match self.position(key) {
            Some(index) => {
                let entry = &mut self.entries[index];
                entry.value = value.over(&entry.value);
            }
            None => self.entries.push(Entry { key, value }),
        }
    }

    /// Sets `key`'s slot as is (a merge's result).
    fn put_slot(&mut self, key: &'static dyn AnySemanticsPropertyKey, value: ValueSlot) {
        match self.position(key) {
            Some(index) => self.entries[index].value = value,
            None => self.entries.push(Entry { key, value }),
        }
    }

    /// The value of `key`, as `getOrNull`.
    pub fn get<T: SemanticsValue>(&self, key: &'static SemanticsPropertyKey<T>) -> Option<&T> {
        self.slot(key).and_then(T::from_slot)
    }

    /// Whether `key` is set, as `contains`.
    pub fn contains<T: SemanticsValue>(&self, key: &'static SemanticsPropertyKey<T>) -> bool {
        self.position(key).is_some()
    }

    /// Sets `key`, as `set`.
    pub fn set<T: SemanticsValue>(&mut self, key: &'static SemanticsPropertyKey<T>, value: T) {
        self.set_slot(key, value.into_slot());
    }

    /// Removes `key`, returning its value.
    pub fn remove<T: SemanticsValue>(
        &mut self,
        key: &'static SemanticsPropertyKey<T>,
    ) -> Option<T> {
        let index = self.position(key)?;
        let entry = self.entries.remove(index);
        T::from_slot(&entry.value).cloned()
    }

    /// Each key and its value, in the order set, as `forEachProperty`.
    pub fn for_each_property(&self, mut f: impl FnMut(&dyn AnySemanticsPropertyKey, &ValueSlot)) {
        for entry in &self.entries {
            f(entry.key, &entry.value);
        }
    }

    /// The keys and values, in the order set.
    pub fn properties(&self) -> impl Iterator<Item = (&dyn AnySemanticsPropertyKey, &ValueSlot)> {
        self.entries.iter().map(|entry| (entry.key, &entry.value))
    }

    /// Whether anything is set (a key, merging or clearing).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
            && !self.is_merging_semantics_of_descendants
            && !self.is_clearing_semantics
    }

    /// Takes in a descendant's properties, as merging does (`mergeChild`): each key by its
    /// policy.
    pub fn merge_child(&mut self, child: &SemanticsConfiguration) {
        for entry in &child.entries {
            let merged = entry.key.merge_slots(self.slot(entry.key), &entry.value);
            if let Some(merged) = merged {
                self.put_slot(entry.key, merged);
            }
        }
    }

    /// Sets what `outer` sets over this one, as an outer modifier's `applySemantics` after the
    /// inner ones' on one node (Compose's `calculateSemanticsConfiguration`): its values win,
    /// merging and clearing add up.
    pub fn apply(&mut self, outer: &SemanticsConfiguration) {
        self.is_merging_semantics_of_descendants |= outer.is_merging_semantics_of_descendants;
        self.is_clearing_semantics |= outer.is_clearing_semantics;
        for entry in &outer.entries {
            self.set_slot(entry.key, entry.value.clone());
        }
    }

    /// The text finders match: the editable text first, then the texts.
    pub fn all_text(&self) -> impl Iterator<Item = &str> {
        let editable = self.get(&SemanticsProperties::EDITABLE_TEXT);
        let texts = self.get(&SemanticsProperties::TEXT);
        editable.into_iter().chain(texts.into_iter().flatten()).map(|text| text.as_str())
    }

    // ---- Compose's `SemanticsPropertyReceiver` ------------------------------------------------

    /// As `contentDescription = …` (one description).
    pub fn set_content_description(&mut self, description: impl Into<SharedString>) {
        self.set(
            &SemanticsProperties::CONTENT_DESCRIPTION,
            SmallVec::from_elem(description.into(), 1),
        );
    }

    pub fn set_hint_text(&mut self, text: impl Into<SharedString>) {
        self.set(&SemanticsProperties::HINT_TEXT, text.into());
    }

    pub fn set_state_description(&mut self, description: impl Into<SharedString>) {
        self.set(&SemanticsProperties::STATE_DESCRIPTION, description.into());
    }

    pub fn set_background_color(&mut self, color: impl Fn() -> Color + 'static) {
        self.set(&SemanticsProperties::BACKGROUND_COLOR, ColorProducer(Rc::new(color)));
    }

    pub fn set_progress_bar_range_info(&mut self, info: ProgressBarRangeInfo) {
        self.set(&SemanticsProperties::PROGRESS_BAR_RANGE_INFO, info);
    }

    /// As `heading()`.
    pub fn heading(&mut self) {
        self.set(&SemanticsProperties::HEADING, ());
    }

    /// As `textEntryKey()`.
    pub fn text_entry_key(&mut self) {
        self.set(&SemanticsProperties::TEXT_ENTRY_KEY, ());
    }

    pub fn set_pane_title(&mut self, title: impl Into<SharedString>) {
        self.set(&SemanticsProperties::PANE_TITLE, title.into());
    }

    /// As `disabled()`.
    pub fn disabled(&mut self) {
        self.set(&SemanticsProperties::DISABLED, ());
    }

    pub fn set_live_region(&mut self, mode: LiveRegionMode) {
        self.set(&SemanticsProperties::LIVE_REGION, mode);
    }

    /// As `focused = …`: set (either way) by what can take the focus.
    pub fn set_focused(&mut self, focused: bool) {
        self.set(&SemanticsProperties::FOCUSED, focused);
    }

    pub fn set_is_traversal_group(&mut self, group: bool) {
        self.set(&SemanticsProperties::IS_TRAVERSAL_GROUP, group);
    }

    pub fn set_is_sensitive_data(&mut self, sensitive: bool) {
        self.set(&SemanticsProperties::IS_SENSITIVE_DATA, sensitive);
    }

    /// As `hideFromAccessibility()`.
    pub fn hide_from_accessibility(&mut self) {
        self.set(&SemanticsProperties::HIDE_FROM_ACCESSIBILITY, ());
    }

    pub fn set_traversal_index(&mut self, index: f32) {
        self.set(&SemanticsProperties::TRAVERSAL_INDEX, index);
    }

    pub fn set_horizontal_scroll_axis_range(&mut self, range: ScrollAxisRange) {
        self.set(&SemanticsProperties::HORIZONTAL_SCROLL_AXIS_RANGE, range);
    }

    pub fn set_vertical_scroll_axis_range(&mut self, range: ScrollAxisRange) {
        self.set(&SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE, range);
    }

    /// As `popup()`.
    pub fn popup(&mut self) {
        self.set(&SemanticsProperties::IS_POPUP, ());
    }

    /// As `dialog()`.
    pub fn dialog(&mut self) {
        self.set(&SemanticsProperties::IS_DIALOG, ());
    }

    pub fn set_role(&mut self, role: Role) {
        self.set(&SemanticsProperties::ROLE, role);
    }

    pub fn set_test_tag(&mut self, tag: impl Into<SharedString>) {
        self.set(&SemanticsProperties::TEST_TAG, tag.into());
    }

    /// As `text = …` (one text).
    pub fn set_text(&mut self, text: impl Into<SharedString>) {
        self.set(&SemanticsProperties::TEXT, SmallVec::from_elem(text.into(), 1));
    }

    pub fn set_text_substitution(&mut self, text: impl Into<SharedString>) {
        self.set(&SemanticsProperties::TEXT_SUBSTITUTION, text.into());
    }

    pub fn set_is_showing_text_substitution(&mut self, showing: bool) {
        self.set(&SemanticsProperties::IS_SHOWING_TEXT_SUBSTITUTION, showing);
    }

    pub fn set_input_text(&mut self, text: impl Into<SharedString>) {
        self.set(&SemanticsProperties::INPUT_TEXT, text.into());
    }

    pub fn set_editable_text(&mut self, text: impl Into<SharedString>) {
        self.set(&SemanticsProperties::EDITABLE_TEXT, text.into());
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.set(&SemanticsProperties::SELECTED, selected);
    }

    pub fn set_collection_info(&mut self, info: CollectionInfo) {
        self.set(&SemanticsProperties::COLLECTION_INFO, info);
    }

    pub fn set_collection_item_info(&mut self, info: CollectionItemInfo) {
        self.set(&SemanticsProperties::COLLECTION_ITEM_INFO, info);
    }

    pub fn set_toggleable_state(&mut self, state: ToggleableState) {
        self.set(&SemanticsProperties::TOGGLEABLE_STATE, state);
    }

    pub fn set_input_text_suggestion_state(&mut self, state: InputTextSuggestionState) {
        self.set(&SemanticsProperties::INPUT_TEXT_SUGGESTION_STATE, state);
    }

    pub fn set_is_editable(&mut self, editable: bool) {
        self.set(&SemanticsProperties::IS_EDITABLE, editable);
    }

    /// As `password()`: the text is a password, shown masked.
    pub fn password(&mut self) {
        self.password_obfuscated(true);
    }

    /// As `password(isPasswordObfuscated)`.
    pub fn password_obfuscated(&mut self, obfuscated: bool) {
        self.set(&SemanticsProperties::PASSWORD, ());
        self.set(&SemanticsProperties::IS_PASSWORD_OBFUSCATED, obfuscated);
    }

    /// As `error(description)`: why its input is invalid.
    pub fn error(&mut self, description: impl Into<SharedString>) {
        self.set(&SemanticsProperties::ERROR, description.into());
    }

    /// As `indexForKey(mapping)`.
    pub fn index_for_key(&mut self, mapping: impl Fn(&dyn Any) -> i32 + 'static) {
        self.set(&SemanticsProperties::INDEX_FOR_KEY, IndexForKey(Rc::new(mapping)));
    }

    pub fn set_max_text_length(&mut self, length: i32) {
        self.set(&SemanticsProperties::MAX_TEXT_LENGTH, length);
    }

    pub fn set_shape(&mut self, shape: Shape) {
        self.set(&SemanticsProperties::SHAPE, shape);
    }

    /// As `selectableGroup()`: it groups selectable items (radio buttons).
    pub fn selectable_group(&mut self) {
        self.set(&SemanticsProperties::SELECTABLE_GROUP, ());
    }

    pub fn set_custom_actions(&mut self, actions: Vec<CustomAccessibilityAction>) {
        self.set(&SemanticsActions::CUSTOM_ACTIONS, actions);
    }

    fn set_action<F: ?Sized + 'static>(
        &mut self,
        key: &'static SemanticsPropertyKey<AccessibilityAction<F>>,
        label: Option<SharedString>,
        action: Rc<F>,
    ) where
        AccessibilityAction<F>: SemanticsValue,
    {
        self.set(key, AccessibilityAction::new(label, action));
    }

    pub fn get_text_layout_result(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn(&mut Vec<TextLayoutResult>) -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::GET_TEXT_LAYOUT_RESULT, label, Rc::new(action));
    }

    pub fn on_click(&mut self, label: Option<SharedString>, action: impl Fn() -> bool + 'static) {
        self.set_action(&SemanticsActions::ON_CLICK, label, Rc::new(action));
    }

    pub fn on_long_click(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn() -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::ON_LONG_CLICK, label, Rc::new(action));
    }

    /// As `scrollBy(label, action)`: `x` and `y` in pixels, towards the content's end.
    pub fn scroll_by(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn(f32, f32) -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::SCROLL_BY, label, Rc::new(action));
    }

    pub fn scroll_by_offset(
        &mut self,
        action: impl Fn(Offset) -> Pin<Box<dyn Future<Output = Offset>>> + 'static,
    ) {
        self.set(&SemanticsActions::SCROLL_BY_OFFSET, ScrollByOffset(Rc::new(action)));
    }

    pub fn scroll_to_index(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn(i32) -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::SCROLL_TO_INDEX, label, Rc::new(action));
    }

    /// As `setProgress(label, action)`: a slider's.
    pub fn set_progress(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn(f32) -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::SET_PROGRESS, label, Rc::new(action));
    }

    /// As `setText(label, action)` (named apart from the `text` setter).
    pub fn set_text_action(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn(AnnotatedString) -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::SET_TEXT, label, Rc::new(action));
    }

    /// As `setTextSubstitution(label, action)` (named apart from the `textSubstitution` setter).
    pub fn set_text_substitution_action(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn(AnnotatedString) -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::SET_TEXT_SUBSTITUTION, label, Rc::new(action));
    }

    pub fn show_text_substitution(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn(bool) -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::SHOW_TEXT_SUBSTITUTION, label, Rc::new(action));
    }

    pub fn clear_text_substitution(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn() -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::CLEAR_TEXT_SUBSTITUTION, label, Rc::new(action));
    }

    pub fn insert_text_at_cursor(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn(AnnotatedString) -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::INSERT_TEXT_AT_CURSOR, label, Rc::new(action));
    }

    /// As `onImeAction(imeActionType, label, action)`.
    pub fn on_ime_action(
        &mut self,
        ime_action: ImeAction,
        label: Option<SharedString>,
        action: impl Fn() -> bool + 'static,
    ) {
        self.set(&SemanticsProperties::IME_ACTION, ime_action);
        self.set_action(&SemanticsActions::ON_IME_ACTION, label, Rc::new(action));
    }

    pub fn set_selection(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn(i32, i32, bool) -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::SET_SELECTION, label, Rc::new(action));
    }

    pub fn copy_text(&mut self, label: Option<SharedString>, action: impl Fn() -> bool + 'static) {
        self.set_action(&SemanticsActions::COPY_TEXT, label, Rc::new(action));
    }

    pub fn cut_text(&mut self, label: Option<SharedString>, action: impl Fn() -> bool + 'static) {
        self.set_action(&SemanticsActions::CUT_TEXT, label, Rc::new(action));
    }

    pub fn paste_text(&mut self, label: Option<SharedString>, action: impl Fn() -> bool + 'static) {
        self.set_action(&SemanticsActions::PASTE_TEXT, label, Rc::new(action));
    }

    pub fn expand(&mut self, label: Option<SharedString>, action: impl Fn() -> bool + 'static) {
        self.set_action(&SemanticsActions::EXPAND, label, Rc::new(action));
    }

    pub fn collapse(&mut self, label: Option<SharedString>, action: impl Fn() -> bool + 'static) {
        self.set_action(&SemanticsActions::COLLAPSE, label, Rc::new(action));
    }

    pub fn dismiss(&mut self, label: Option<SharedString>, action: impl Fn() -> bool + 'static) {
        self.set_action(&SemanticsActions::DISMISS, label, Rc::new(action));
    }

    pub fn request_focus(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn() -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::REQUEST_FOCUS, label, Rc::new(action));
    }

    /// compose-rs's: offers `RequestFocus`, moving the focus to this node's focus target (or
    /// the first below it), as Compose's `focusable` does through its requester. The tree
    /// binds it to the node when it builds the configuration (a node's semantics don't know
    /// their node yet, D1251).
    pub fn request_focus_of_node(&mut self) {
        self.set(
            &SemanticsActions::REQUEST_FOCUS,
            AccessibilityAction { label: None, action: None },
        );
    }

    pub fn page_up(&mut self, label: Option<SharedString>, action: impl Fn() -> bool + 'static) {
        self.set_action(&SemanticsActions::PAGE_UP, label, Rc::new(action));
    }

    pub fn page_down(&mut self, label: Option<SharedString>, action: impl Fn() -> bool + 'static) {
        self.set_action(&SemanticsActions::PAGE_DOWN, label, Rc::new(action));
    }

    pub fn page_left(&mut self, label: Option<SharedString>, action: impl Fn() -> bool + 'static) {
        self.set_action(&SemanticsActions::PAGE_LEFT, label, Rc::new(action));
    }

    pub fn page_right(&mut self, label: Option<SharedString>, action: impl Fn() -> bool + 'static) {
        self.set_action(&SemanticsActions::PAGE_RIGHT, label, Rc::new(action));
    }

    pub fn get_scroll_viewport_length(
        &mut self,
        label: Option<SharedString>,
        action: impl Fn(&mut Vec<f32>) -> bool + 'static,
    ) {
        self.set_action(&SemanticsActions::GET_SCROLL_VIEWPORT_LENGTH, label, Rc::new(action));
    }

    /// compose-rs's: the properties belong to the nearest ancestor with semantics
    /// ([`PART_OF_PARENT`]).
    pub fn part_of_parent(&mut self) {
        self.set(&PART_OF_PARENT, ());
    }
}

impl fmt::Debug for SemanticsConfiguration {
    /// One line: the common properties first (as the semantics tree's dump always printed
    /// them), then the others by name.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SemanticsActions as A;
        use SemanticsProperties as P;
        let mut parts: Vec<String> = Vec::new();
        if let Some(role) = self.get(&P::ROLE) {
            parts.push(format!("role={role:?}"));
        }
        if let Some(text) = self.get(&P::TEXT) {
            parts.push(format!("text={:?}", text.iter().map(|t| t.as_str()).collect::<Vec<_>>()));
        }
        if let Some(text) = self.get(&P::EDITABLE_TEXT) {
            parts.push(format!("editable_text={text:?}"));
        }
        if let Some(error) = self.get(&P::ERROR) {
            parts.push(format!("error={error:?}"));
        }
        if let Some(descriptions) = self.get(&P::CONTENT_DESCRIPTION) {
            parts.push(format!(
                "content_description={:?}",
                descriptions.iter().map(|t| t.as_str()).collect::<Vec<_>>()
            ));
        }
        if let Some(tag) = self.get(&P::TEST_TAG) {
            parts.push(format!("tag={tag:?}"));
        }
        if let Some(state) = self.get(&P::TOGGLEABLE_STATE) {
            parts.push(format!("toggle={state:?}"));
        }
        if let Some(selected) = self.get(&P::SELECTED) {
            parts.push(format!("selected={selected}"));
        }
        if let Some(state) = self.get(&P::STATE_DESCRIPTION) {
            parts.push(format!("state={:?}", state.as_str()));
        }
        if let Some(info) = self.get(&P::PROGRESS_BAR_RANGE_INFO) {
            parts.push(format!(
                "progress={} in {}..{} steps {}",
                info.current, info.range.0, info.range.1, info.steps
            ));
        }
        for (key, axis) in [
            (&P::HORIZONTAL_SCROLL_AXIS_RANGE, "horizontal"),
            (&P::VERTICAL_SCROLL_AXIS_RANGE, "vertical"),
        ] {
            if let Some(range) = self.get(key) {
                let reverse = if range.reverse_scrolling { " reversed" } else { "" };
                parts.push(format!("{axis}_scroll={}/{}{reverse}", range.value, range.max_value));
            }
        }
        let focused = self.get(&P::FOCUSED);
        for (flag, name) in [
            (self.contains(&A::ON_CLICK), "clickable"),
            (self.contains(&A::ON_LONG_CLICK), "long_clickable"),
            (self.get(&P::IS_EDITABLE) == Some(&true), "editable"),
            (self.contains(&P::PASSWORD), "password"),
            (self.contains(&P::DISABLED), "disabled"),
            (focused.is_some(), "focusable"),
            (focused == Some(&true), "focused"),
            (self.is_merging_semantics_of_descendants, "merged"),
            (self.contains(&PART_OF_PARENT), "part_of_parent"),
            (self.contains(&P::IS_DIALOG), "dialog"),
            (self.contains(&P::IS_POPUP), "popup"),
            (self.is_clearing_semantics, "cleared"),
            (self.contains(&P::SELECTABLE_GROUP), "selectable_group"),
        ] {
            if flag {
                parts.push(name.to_string());
            }
        }
        // The others, by Compose's names.
        let shown: [&'static dyn AnySemanticsPropertyKey; 21] = [
            &P::ROLE,
            &P::TEXT,
            &P::EDITABLE_TEXT,
            &P::ERROR,
            &P::CONTENT_DESCRIPTION,
            &P::TEST_TAG,
            &P::TOGGLEABLE_STATE,
            &P::SELECTED,
            &P::STATE_DESCRIPTION,
            &P::PROGRESS_BAR_RANGE_INFO,
            &P::HORIZONTAL_SCROLL_AXIS_RANGE,
            &P::VERTICAL_SCROLL_AXIS_RANGE,
            &A::ON_CLICK,
            &A::ON_LONG_CLICK,
            &P::IS_EDITABLE,
            &P::PASSWORD,
            &P::DISABLED,
            &P::FOCUSED,
            &PART_OF_PARENT,
            &P::IS_DIALOG,
            &P::IS_POPUP,
        ];
        for entry in &self.entries {
            // Actions besides the clicks aren't printed (what offers them is: a scroll range,
            // a focusable node…), nor the password's obfuscation.
            let common = same_key(entry.key, &P::SELECTABLE_GROUP)
                || same_key(entry.key, &P::IS_PASSWORD_OBFUSCATED)
                || entry.value.is_action()
                || shown.iter().any(|key| same_key(*key, entry.key));
            if !common {
                parts.push(format!("{}={:?}", entry.key.name(), entry.value));
            }
        }
        formatter.write_str(&parts.join(" "))
    }
}
