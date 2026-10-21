//! `Popup` and `Dialog`, as Compose's `androidx.compose.ui.window`: content over the rest of the
//! window, on overlays (`crate::overlay`).

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::{Callback, composable, remember};

use crate::alignment::Alignment;
use crate::constraints::Constraints;
use crate::graphics::Color;
use crate::input::{
    EventContext, Key, KeyEvent, KeyEventType, PointerEvent, PointerEventPass, PointerEventType,
};
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
    fn position(&self, anchor: Rect, _window: IntSize, popup: IntSize) -> IntOffset {
        let direction = crate::alignment::LayoutDirection::Ltr;
        let anchor_size =
            IntSize::new(anchor.width().round() as i32, anchor.height().round() as i32);
        let in_anchor = self.alignment.align(IntSize::new(0, 0), anchor_size, direction);
        let in_popup = self.alignment.align(IntSize::new(0, 0), popup, direction);
        IntOffset::new(
            anchor.left.round() as i32 + in_anchor.x - in_popup.x + self.offset.x,
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
    fn position(&self, anchor: Rect, window: IntSize, popup: IntSize) -> IntOffset {
        self.0.get().position(anchor, window, popup)
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
            // Under the popup, the whole window: a click there dismisses it.
            if properties.dismiss_on_click_outside
                && let Some(dismiss) = on_dismiss_request.clone()
            {
                let catcher = Modifier.overlay(window.clone(), false).then_custom(DismissOnClick {
                    on_dismiss: dismiss,
                    inside: Rc::new(Cell::new(None)),
                });
                Layout(catcher, fill.clone(), compose_runtime::callback(|| {}));
            }
            let mut modifier = Modifier.overlay(position_provider.clone(), properties.focusable);
            if properties.focusable {
                modifier = modifier.focusable(true);
            }
            if properties.dismiss_on_back_press
                && let Some(dismiss) = on_dismiss_request.clone()
            {
                modifier = modifier
                    .on_key_event(move |event: KeyEvent| dismiss_on_escape(&event, &dismiss));
            }
            let content = content.clone();
            Layout(modifier, stack.clone(), compose_runtime::callback(move || content()));
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
            let mut modifier = Modifier
                .overlay(window.clone(), true)
                .background(SCRIM)
                .then_custom(DismissOnClick {
                    on_dismiss: if properties.dismiss_on_click_outside {
                        dismiss.clone()
                    } else {
                        compose_runtime::callback(|| {})
                    },
                    inside: inside.clone(),
                })
                .focusable(true);
            if properties.dismiss_on_back_press {
                let dismiss = dismiss.clone();
                modifier = modifier
                    .on_key_event(move |event: KeyEvent| dismiss_on_escape(&event, &dismiss));
            }
            let content = content.clone();
            Layout(modifier, center.clone(), compose_runtime::callback(move || content()));
        }),
    );
}

/// The dim behind a dialog, as Material's scrim (black at 32 %).
const SCRIM: Color = Color::from_argb(0x52000000);

fn dismiss_on_escape(event: &KeyEvent, dismiss: &Callback) -> bool {
    if event.key == Key::Escape && event.kind == KeyEventType::KeyDown {
        dismiss();
        true
    } else {
        false
    }
}

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

/// Takes every pointer event (what's beneath gets none), and calls `on_dismiss` for a click
/// outside `inside` (all of it if `None`).
struct DismissOnClick {
    on_dismiss: Callback,
    inside: Rc<Cell<Option<Rect>>>,
}

struct DismissOnClickNode {
    on_dismiss: Callback,
    inside: Rc<Cell<Option<Rect>>>,
    pressed_outside: bool,
}

impl ModifierNodeElement for DismissOnClick {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(DismissOnClickNode {
            on_dismiss: self.on_dismiss.clone(),
            inside: self.inside.clone(),
            pressed_outside: false,
        })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<DismissOnClickNode>() {
            node.on_dismiss = self.on_dismiss.clone();
            node.inside = self.inside.clone();
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<DismissOnClick>().is_some_and(|other| {
            other.on_dismiss.ptr_eq(&self.on_dismiss) && Rc::ptr_eq(&other.inside, &self.inside)
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
        if pass != PointerEventPass::Main {
            return;
        }
        let outside = self.inside.get().is_none_or(|inside| !inside.contains(event.position));
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
        event.consume();
    }

    fn on_cancel_pointer_input(&mut self) {
        self.pressed_outside = false;
    }
}
