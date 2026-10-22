//! `Popup` and `Dialog`, as Compose's `androidx.compose.ui.window`: content over the rest of the
//! window, on overlays (`crate::overlay`).

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::{Callback, composable, remember};

use crate::alignment::Alignment;
use crate::constraints::Constraints;
use crate::graphics::Color;
use crate::input::{EventContext, PointerEvent, PointerEventPass, PointerEventType};
use crate::layout::{Layout, Measurable, MeasurePolicy, MeasureResult, MeasureScope, Placeable};
use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement, PointerInputModifierNode};
use crate::overlay::{PopupPositionProvider, WindowPosition};
use crate::unit::{IntOffset, IntSize, Rect};

/// How a popup behaves, as Compose's `PopupProperties`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PopupProperties {
    /// The popup takes the focus when it opens (a menu), and Escape dismisses it.
    pub focusable: bool,
    /// Escape dismisses it, as `dismissOnBackPress`.
    pub dismiss_on_back_press: bool,
    /// A click outside dismisses it.
    pub dismiss_on_click_outside: bool,
}

impl Default for PopupProperties {
    fn default() -> PopupProperties {
        PopupProperties {
            focusable: false,
            dismiss_on_back_press: true,
            dismiss_on_click_outside: true,
        }
    }
}

impl compose_runtime::ComposeArg for PopupProperties {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// How a dialog behaves, as Compose's `DialogProperties`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DialogProperties {
    pub dismiss_on_back_press: bool,
    pub dismiss_on_click_outside: bool,
}

impl Default for DialogProperties {
    fn default() -> DialogProperties {
        DialogProperties { dismiss_on_back_press: true, dismiss_on_click_outside: true }
    }
}

impl compose_runtime::ComposeArg for DialogProperties {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Positions a popup by aligning it with its anchor, as Compose's
/// `AlignmentOffsetPositionProvider`: `alignment`'s point of the popup on the same point of the
/// anchor, moved by `offset`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AlignmentOffsetPositionProvider {
    pub alignment: Alignment,
    pub offset: IntOffset,
}

impl PopupPositionProvider for AlignmentOffsetPositionProvider {
    fn position(
        &self,
        anchor: Rect,
        _window: IntSize,
        direction: crate::alignment::LayoutDirection,
        popup: IntSize,
    ) -> IntOffset {
        let anchor_size =
            IntSize::new(anchor.width().round() as i32, anchor.height().round() as i32);
        let in_anchor = self.alignment.align(IntSize::new(0, 0), anchor_size, direction);
        let in_popup = self.alignment.align(IntSize::new(0, 0), popup, direction);
        // The offset goes toward the end: left in a right-to-left layout.
        let offset_x = match direction {
            crate::alignment::LayoutDirection::Ltr => self.offset.x,
            crate::alignment::LayoutDirection::Rtl => -self.offset.x,
        };
        IntOffset::new(
            anchor.left.round() as i32 + in_anchor.x - in_popup.x + offset_x,
            anchor.top.round() as i32 + in_anchor.y - in_popup.y + self.offset.y,
        )
    }
}

/// Content over the rest of the window, aligned with the layout around it, as Compose's
/// `Popup(alignment, offset, onDismissRequest, properties, content)`.
#[composable]
#[allow(non_snake_case)]
pub fn Popup(
    #[default(crate::Alignment::TOP_START)] alignment: Alignment,
    #[default(crate::IntOffset::ZERO)] offset: IntOffset,
    #[default(None)] on_dismiss_request: Option<impl Fn() + 'static>,
    #[default(crate::window::PopupProperties::default())] properties: PopupProperties,
    content: impl Fn() + 'static,
) {
    let provider =
        remember(|| Rc::new(Cell::new(AlignmentOffsetPositionProvider { alignment, offset })));
    provider.set(AlignmentOffsetPositionProvider { alignment, offset });
    let provider =
        remember(|| Rc::new(SharedProvider(provider.clone())) as Rc<dyn PopupPositionProvider>);
    PopupWithPositionProvider(provider, on_dismiss_request, properties, content);
}

/// A provider read from a cell, so the popup keeps one provider (and its overlay node) while
/// its alignment and offset change.
struct SharedProvider(Rc<Cell<AlignmentOffsetPositionProvider>>);

impl PopupPositionProvider for SharedProvider {
    fn position(
        &self,
        anchor: Rect,
        window: IntSize,
        direction: crate::alignment::LayoutDirection,
        popup: IntSize,
    ) -> IntOffset {
        self.0.get().position(anchor, window, direction, popup)
    }
}

/// A popup placed by `position_provider`, as Compose's `Popup(popupPositionProvider, …)`.
/// With `on_dismiss_request`, a click outside it (if `dismiss_on_click_outside`) or Escape (if
/// focused and `dismiss_on_back_press`) calls it.
#[composable]
#[allow(non_snake_case)]
pub fn PopupWithPositionProvider(
    position_provider: Rc<dyn PopupPositionProvider>,
    #[default(None)] on_dismiss_request: Option<impl Fn() + 'static>,
    #[default(crate::window::PopupProperties::default())] properties: PopupProperties,
    content: impl Fn() + 'static,
) {
    let host = remember(|| Rc::new(HostLayout) as Rc<dyn MeasurePolicy>);
    let stack = remember(|| Rc::new(StackLayout) as Rc<dyn MeasurePolicy>);
    let fill = remember(|| Rc::new(FillLayout) as Rc<dyn MeasurePolicy>);
    let window = remember(|| Rc::new(WindowPosition) as Rc<dyn PopupPositionProvider>);
    Layout(
        Modifier,
        host,
        compose_runtime::callback(move || {
            // Under the popup, the whole window: a press there dismisses it. A focusable popup
            // takes the press (what's beneath gets nothing while it shows); another lets it
            // through, as Android's non-focusable popup windows (`FLAG_NOT_TOUCH_MODAL`).
            // A focusable popup's outside is there even when a press doesn't dismiss it.
            let dismiss =
                on_dismiss_request.clone().filter(|_| properties.dismiss_on_click_outside);
            if properties.focusable || dismiss.is_some() {
                let outside = if properties.focusable {
                    Modifier.overlay(window.clone(), false)
                } else {
                    Modifier.overlay_passing_through(window.clone())
                };
                let catcher = outside.then_custom(DismissOnClick {
                    on_dismiss: dismiss.unwrap_or_else(|| compose_runtime::callback(|| {})),
                    inside: Rc::new(Cell::new(None)),
                    on_press: true,
                    modal: properties.focusable,
                });
                Layout(catcher, fill.clone(), compose_runtime::callback(|| {}));
            }
            let mut modifier = Modifier.overlay(position_provider.clone(), properties.focusable);
            if properties.focusable {
                modifier = modifier.focusable(true);
            }
            // A focusable popup takes back (Escape on desktop) before what's beneath, as on
            // Android a popup window that takes focus gets the back key, and dismisses if
            // `dismiss_on_back_press`; back never goes past it while it shows.
            let layer = crate::back::LOCAL_BACK_LAYER.current() + u32::from(properties.focusable);
            if properties.focusable {
                let on_back =
                    back_dismissal(on_dismiss_request.clone(), properties.dismiss_on_back_press);
                compose_runtime::CompositionLocalProvider(
                    crate::back::LOCAL_BACK_LAYER.provides(layer),
                    move || crate::back::BackHandler(true, on_back),
                );
            }
            let content = content.clone();
            compose_runtime::CompositionLocalProvider(
                crate::back::LOCAL_BACK_LAYER.provides(layer),
                move || {
                    Layout(modifier, stack.clone(), compose_runtime::callback(move || content()))
                },
            );
        }),
    );
}

/// A modal dialog, as Compose's `Dialog`: its content centered over a scrim covering the
/// window, taking the focus; a click on the scrim (if `dismiss_on_click_outside`) or Escape (if
/// `dismiss_on_back_press`) calls `on_dismiss_request`. What's beneath gets no input.
#[composable]
#[allow(non_snake_case)]
pub fn Dialog(
    on_dismiss_request: impl Fn() + 'static,
    #[default(crate::window::DialogProperties::default())] properties: DialogProperties,
    content: impl Fn() + 'static,
) {
    let host = remember(|| Rc::new(HostLayout) as Rc<dyn MeasurePolicy>);
    let stack = remember(|| Rc::new(StackLayout) as Rc<dyn MeasurePolicy>);
    let window = remember(|| Rc::new(WindowPosition) as Rc<dyn PopupPositionProvider>);
    let inside = remember(|| Rc::new(Cell::new(None::<Rect>)));
    let center = {
        let inside = inside.clone();
        remember(move || Rc::new(CenterLayout { inside }) as Rc<dyn MeasurePolicy>)
    };
    Layout(
        Modifier,
        host,
        compose_runtime::callback(move || {
            let dismiss = on_dismiss_request.clone();
            let modifier = Modifier
                .overlay(window.clone(), true)
                .background(SCRIM)
                .then_custom(DismissOnClick {
                    on_dismiss: if properties.dismiss_on_click_outside {
                        dismiss.clone()
                    } else {
                        compose_runtime::callback(|| {})
                    },
                    inside: inside.clone(),
                    on_press: false,
                    modal: true,
                })
                .focusable(true);
            // A dialog is a window of its own: back (Escape on desktop) goes to it, and to the
            // handlers in it, before what's beneath, and never past it: it takes back while
            // shown, dismissing only if `dismiss_on_back_press`, as Compose's dialog window.
            let layer = crate::back::LOCAL_BACK_LAYER.current() + 1;
            let on_back = back_dismissal(Some(dismiss.clone()), properties.dismiss_on_back_press);
            compose_runtime::CompositionLocalProvider(
                crate::back::LOCAL_BACK_LAYER.provides(layer),
                move || crate::back::BackHandler(true, on_back),
            );
            let content = content.clone();
            let stack = stack.clone();
            // The dialog's own node, as large as its content: what's marked a dialog.
            let content = compose_runtime::callback(move || {
                let content = content.clone();
                Layout(
                    Modifier.semantics(false, |config| config.is_dialog = true),
                    stack.clone(),
                    compose_runtime::callback(move || content()),
                );
            });
            compose_runtime::CompositionLocalProvider(
                crate::back::LOCAL_BACK_LAYER.provides(layer),
                move || Layout(modifier, center.clone(), content),
            );
        }),
    );
}

/// The dim behind a dialog, as Material's scrim (black at 32 %).
const SCRIM: Color = Color::from_argb(0x52000000);

/// A popup's host in the tree: no size; its overlays are measured with the window's.
struct HostLayout;

impl MeasurePolicy for HostLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        _constraints: Constraints,
    ) -> MeasureResult {
        let window = scope.window_size();
        let constraints = Constraints::new(0, window.width, 0, window.height);
        let placeables: Vec<Placeable> =
            measurables.iter().map(|&child| scope.measure(child, constraints)).collect();
        for placeable in &placeables {
            scope.place(placeable, 0, 0);
        }
        scope.layout(0, 0)
    }

    fn debug_name(&self) -> &'static str {
        "PopupHost"
    }
}

/// Children stacked at the top start, as large as the largest.
struct StackLayout;

impl MeasurePolicy for StackLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let placeables: Vec<Placeable> =
            measurables.iter().map(|&child| scope.measure(child, constraints)).collect();
        let size = placeables.iter().fold(
            IntSize::new(constraints.min_width, constraints.min_height),
            |size, placeable| {
                IntSize::new(size.width.max(placeable.width), size.height.max(placeable.height))
            },
        );
        for placeable in &placeables {
            scope.place(placeable, 0, 0);
        }
        scope.layout(size.width, size.height)
    }

    fn debug_name(&self) -> &'static str {
        "Popup"
    }
}

/// As large as allowed (the window).
struct FillLayout;

impl MeasurePolicy for FillLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        scope.layout(constraints.max_width, constraints.max_height)
    }

    fn debug_name(&self) -> &'static str {
        "PopupOutside"
    }
}

/// The window, its children centered; records where they are (what isn't the outside).
struct CenterLayout {
    inside: Rc<Cell<Option<Rect>>>,
}

impl MeasurePolicy for CenterLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let (width, height) = (constraints.max_width, constraints.max_height);
        // 24 dp from the window's edges, as a platform dialog's default width keeps.
        let margin = scope.density().round_to_px(crate::unit::Dp(24.0));
        let loose =
            Constraints::new(0, (width - 2 * margin).max(0), 0, (height - 2 * margin).max(0));
        let placeables: Vec<Placeable> =
            measurables.iter().map(|&child| scope.measure(child, loose)).collect();
        let mut inside: Option<Rect> = None;
        for placeable in &placeables {
            let x = (width - placeable.width) / 2;
            let y = (height - placeable.height) / 2;
            scope.place(placeable, x, y);
            let rect = Rect::new(
                x as f32,
                y as f32,
                (x + placeable.width) as f32,
                (y + placeable.height) as f32,
            );
            inside = Some(match inside {
                Some(union) => Rect::new(
                    union.left.min(rect.left),
                    union.top.min(rect.top),
                    union.right.max(rect.right),
                    union.bottom.max(rect.bottom),
                ),
                None => rect,
            });
        }
        self.inside.set(inside);
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "Dialog"
    }
}

/// Calls `on_dismiss` for a press (`on_press`, a popup's) or a click (a dialog's) outside
/// `inside` (all of it if `None`); `modal`, it takes every pointer event (what's beneath gets
/// none).
struct DismissOnClick {
    on_dismiss: Callback,
    inside: Rc<Cell<Option<Rect>>>,
    on_press: bool,
    modal: bool,
}

struct DismissOnClickNode {
    on_dismiss: Callback,
    inside: Rc<Cell<Option<Rect>>>,
    on_press: bool,
    modal: bool,
    pressed_outside: bool,
}

impl ModifierNodeElement for DismissOnClick {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(DismissOnClickNode {
            on_dismiss: self.on_dismiss.clone(),
            inside: self.inside.clone(),
            on_press: self.on_press,
            modal: self.modal,
            pressed_outside: false,
        })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<DismissOnClickNode>() {
            node.on_dismiss = self.on_dismiss.clone();
            node.inside = self.inside.clone();
            node.on_press = self.on_press;
            node.modal = self.modal;
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<DismissOnClick>().is_some_and(|other| {
            other.on_dismiss.ptr_eq(&self.on_dismiss)
                && Rc::ptr_eq(&other.inside, &self.inside)
                && other.on_press == self.on_press
                && other.modal == self.modal
        })
    }
    fn name(&self) -> &'static str {
        "dismiss_on_click_outside"
    }
}

impl ModifierNode for DismissOnClickNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for DismissOnClickNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        let outside = self.inside.get().is_none_or(|inside| !inside.contains(event.position));
        // A popup's press outside dismisses it before anything beneath handles it.
        if self.on_press {
            if pass == PointerEventPass::Initial
                && event.kind == PointerEventType::Press
                && outside
                && !event.consumed
            {
                context.call(&self.on_dismiss);
            }
            if self.modal && pass == PointerEventPass::Main {
                event.consume();
            }
            return;
        }
        if pass != PointerEventPass::Main {
            return;
        }
        match event.kind {
            PointerEventType::Press => self.pressed_outside = outside && !event.consumed,
            PointerEventType::Release
                if std::mem::take(&mut self.pressed_outside) && outside && !event.consumed =>
            {
                context.call(&self.on_dismiss);
            }
            _ => {}
        }
        // Modal: nothing beneath gets it.
        if self.modal {
            event.consume();
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.pressed_outside = false;
    }
}

/// What back does in a dialog or a focusable popup: dismiss it if it may (`dismiss`), else
/// nothing (it still takes back from what's beneath).
fn back_dismissal(dismiss: Option<Callback>, dismiss_on_back_press: bool) -> Callback {
    match dismiss.filter(|_| dismiss_on_back_press) {
        Some(dismiss) => dismiss,
        None => compose_runtime::callback(|| {}),
    }
}
