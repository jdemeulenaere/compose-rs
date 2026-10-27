//! `ModalBottomSheet`, as Compose's Material 3: a sheet sliding up from the bottom of the
//! window over a scrim, dragged between hidden, partially expanded and expanded, with its
//! `SheetState`.

use compose_runtime::{ObjectHandle, Owned};
use std::rc::Rc;

use compose_animation::{AnimationSpec, FAST_OUT_SLOW_IN_EASING, Spring, tween};
use compose_foundation::gestures::{DraggableModifier, DraggableState, Orientation};
use compose_foundation::layout::{Box, Column, ColumnScope};
use compose_runtime::{ComposeArg, LaunchedEffect, composable, remember, remember_coroutine_scope};
use compose_ui::{
    Alignment, Color, Constraints, Dp, IntOffset, IntSize, Layout, LayoutDirection, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, PaddingValues, PopupPositionProvider,
    PopupProperties, Rect, Shape, UnitExt,
};
use compose_ui::{NestedScrollConnection, NestedScrollSource, Offset};

use crate::anchored::Anchored;
use crate::theme::MaterialTheme;

/// Where a sheet rests, as Compose's `SheetValue`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SheetValue {
    Hidden,
    Expanded,
    PartiallyExpanded,
}

impl SheetValue {
    fn from_key(key: u8) -> SheetValue {
        match key {
            0 => SheetValue::Hidden,
            1 => SheetValue::Expanded,
            _ => SheetValue::PartiallyExpanded,
        }
    }
}

impl ComposeArg for SheetValue {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

struct SheetInner {
    anchored: Anchored,
    skip_partially_expanded: bool,
    skip_hidden_state: bool,
}

/// A bottom sheet's state, as Compose's `SheetState`: where it rests and where it's going,
/// moved by `show`, `hide`, `expand` and `partial_expand` (animated) and by drags.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_modal_bottom_sheet_state`] keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SheetState {
    object: ObjectHandle<SheetInner>,
}

impl SheetState {
    /// Its object, or `None` once its owner went (D240): then it does nothing, its reads
    /// giving the defaults.
    #[track_caller]
    fn inner(&self) -> Option<Rc<SheetInner>> {
        self.object.get()
    }

    /// As `SheetState(skipPartiallyExpanded, initialValue, confirmValueChange, skipHiddenState)`:
    /// `confirm_value_change` can refuse where a drag would settle.
    #[track_caller]
    pub fn new(
        skip_partially_expanded: bool,
        initial_value: SheetValue,
        confirm_value_change: impl Fn(SheetValue) -> bool + 'static,
        skip_hidden_state: bool,
    ) -> Owned<SheetState> {
        // A 300 ms ease, as Compose's `BottomSheetAnimationSpec`.
        let spec = AnimationSpec::from(tween(300).easing(FAST_OUT_SLOW_IN_EASING));
        Owned::new(
            SheetInner {
                anchored: Anchored::new(
                    initial_value as u8,
                    Box::new(move |key| confirm_value_change(SheetValue::from_key(key))),
                    spec,
                ),
                skip_partially_expanded,
                skip_hidden_state,
            },
            |object| SheetState { object },
        )
    }

    /// Where it last settled (a state read), as `currentValue`.
    pub fn current_value(&self) -> SheetValue {
        self.inner().map_or(SheetValue::Hidden, |inner| {
            SheetValue::from_key(inner.anchored.current_value())
        })
    }

    /// Where it's going (a state read), as `targetValue`.
    pub fn target_value(&self) -> SheetValue {
        self.inner()
            .map_or(SheetValue::Hidden, |inner| SheetValue::from_key(inner.anchored.target_value()))
    }

    /// Whether it's showing, as `isVisible`.
    pub fn is_visible(&self) -> bool {
        self.current_value() != SheetValue::Hidden
    }

    /// Whether it can rest expanded, as `hasExpandedState`.
    pub fn has_expanded_state(&self) -> bool {
        self.inner().is_some_and(|inner| inner.anchored.has_anchor(SheetValue::Expanded as u8))
    }

    /// Whether it can rest half way, as `hasPartiallyExpandedState`.
    pub fn has_partially_expanded_state(&self) -> bool {
        self.inner()
            .is_some_and(|inner| inner.anchored.has_anchor(SheetValue::PartiallyExpanded as u8))
    }

    /// Its top's offset from the top of the window, in pixels (a state read; NaN before it's
    /// laid out), as `requireOffset()`.
    pub fn require_offset(&self) -> f32 {
        self.inner().map_or(f32::NAN, |inner| inner.anchored.offset())
    }

    /// Shows it, half way if it can rest there, as `show()`.
    pub async fn show(&self) {
        let value = if self.has_partially_expanded_state() {
            SheetValue::PartiallyExpanded
        } else {
            SheetValue::Expanded
        };
        if let Some(inner) = self.inner() {
            inner.anchored.animate_to(value as u8).await;
        }
    }

    /// Hides it, as `hide()` (not with `skip_hidden_state`).
    pub async fn hide(&self) {
        if let Some(inner) = self.inner()
            && !inner.skip_hidden_state
        {
            inner.anchored.animate_to(SheetValue::Hidden as u8).await;
        }
    }

    /// Expands it, as `expand()`.
    pub async fn expand(&self) {
        if let Some(inner) = self.inner() {
            inner.anchored.animate_to(SheetValue::Expanded as u8).await;
        }
    }

    /// Takes it half way, as `partialExpand()` (not with `skip_partially_expanded`).
    pub async fn partial_expand(&self) {
        if let Some(inner) = self.inner()
            && !inner.skip_partially_expanded
        {
            inner.anchored.animate_to(SheetValue::PartiallyExpanded as u8).await;
        }
    }

    /// At `value` at once, as `snapTo`.
    pub fn snap_to(&self, value: SheetValue) {
        if let Some(inner) = self.inner() {
            inner.anchored.snap_to(value as u8);
        }
    }
}

impl ComposeArg for SheetState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A modal sheet's state remembered here, hidden, as `rememberModalBottomSheetState(
/// skipPartiallyExpanded)`.
#[composable(non_restartable)]
pub fn remember_modal_bottom_sheet_state(
    #[default(false)] skip_partially_expanded: bool,
) -> SheetState {
    remember(|| SheetState::new(skip_partially_expanded, SheetValue::Hidden, |_| true, false))
}

/// As Compose's `BottomSheetDefaults`.
pub struct BottomSheetDefaults;

impl BottomSheetDefaults {
    /// The widest a sheet gets, as `SheetMaxWidth`.
    pub const SHEET_MAX_WIDTH: Dp = Dp(640.0);
    /// As `SheetPeekHeight`.
    pub const SHEET_PEEK_HEIGHT: Dp = Dp(56.0);

    /// Extra-large top corners, as `ExpandedShape`.
    pub fn expanded_shape() -> Shape {
        let corner = match MaterialTheme::shapes().extra_large.corners() {
            Some(corners) => corners[0],
            None => compose_ui::CornerSize::Dp(Dp(0.0)),
        };
        let none = compose_ui::CornerSize::Dp(Dp(0.0));
        Shape::Rounded([corner, corner, none, none])
    }

    /// The low surface container, as `ContainerColor`.
    pub fn container_color() -> Color {
        MaterialTheme::color_scheme().surface_container_low
    }

    /// 32 % of the scrim color, as `ScrimColor`.
    pub fn scrim_color() -> Color {
        MaterialTheme::color_scheme().scrim.with_alpha(0.32)
    }

    /// As `windowInsets`: none until hosts report them.
    pub fn window_insets() -> PaddingValues {
        PaddingValues::ZERO
    }

    /// The drag handle, as `DragHandle()`: a 32 by 4 dp pill in the on-surface-variant color
    /// at 40 %, 22 dp above and below.
    pub fn drag_handle() {
        let color = MaterialTheme::color_scheme().on_surface_variant.with_alpha(0.4);
        crate::components::Surface(
            Modifier.padding((Dp(0.0), 22.dp())).semantics(|config| {
                config.set_content_description("Drag handle");
            }),
            MaterialTheme::shapes().extra_large,
            color,
            Color::UNSPECIFIED,
            Dp(0.0),
            Dp(0.0),
            None,
            compose_runtime::callback(|| {
                compose_foundation::layout::Spacer(Modifier.size((32.dp(), 4.dp())));
            }),
        );
    }
}

/// The sheet's part in its content's scrolls.
struct SheetConnection {
    state: SheetState,
    settle: std::cell::RefCell<Rc<dyn Fn(f32)>>,
}

impl NestedScrollConnection for SheetConnection {
    /// Scrolling the content up drags the sheet up first, as far as it goes.
    fn on_pre_scroll(&self, available: Offset, source: NestedScrollSource) -> Offset {
        if available.y < 0.0
            && source == NestedScrollSource::UserInput
            && let Some(inner) = self.state.inner()
        {
            Offset::new(0.0, inner.anchored.drag_by(available.y))
        } else {
            Offset::ZERO
        }
    }

    /// What the content leaves scrolling down (it's at its top) drags the sheet down.
    fn on_post_scroll(&self, _: Offset, available: Offset, source: NestedScrollSource) -> Offset {
        if source == NestedScrollSource::UserInput
            && let Some(inner) = self.state.inner()
        {
            Offset::new(0.0, inner.anchored.drag_by(available.y))
        } else {
            Offset::ZERO
        }
    }

    /// A fling up while the sheet isn't at its top settles the sheet instead.
    fn on_pre_fling(&self, available: Offset) -> Offset {
        let Some(inner) = self.state.inner() else { return Offset::ZERO };
        let anchored = &inner.anchored;
        if available.y < 0.0 && anchored.offset() > anchored.bounds().0 {
            (self.settle.borrow())(available.y);
            available
        } else {
            Offset::ZERO
        }
    }

    /// After the content's fling (or a drag let go), the sheet settles.
    fn on_post_fling(&self, _: Offset, available: Offset) -> Offset {
        (self.settle.borrow())(available.y);
        available
    }
}

/// The window's top start, for the sheet's overlay.
struct AtWindowTop;

impl PopupPositionProvider for AtWindowTop {
    fn position(&self, _: Rect, _: IntSize, _: LayoutDirection, _: IntSize) -> IntOffset {
        IntOffset::ZERO
    }
}

/// How far a drag must go to settle at the next anchor, as Compose's sheets' positional
/// threshold.
const POSITIONAL_THRESHOLD: Dp = Dp(56.0);
/// A release faster than this settles in its direction, as Compose's sheets' velocity
/// threshold (125 dp/s).
const VELOCITY_THRESHOLD: Dp = Dp(125.0);

/// A modal bottom sheet, as Compose's `ModalBottomSheet`: over the whole window, a scrim (a
/// click on it hides the sheet) and the sheet at the bottom (up to 640 dp wide, centered), its
/// drag handle and content in a column. It shows as it enters (half way if it's taller than
/// half the window), and drags between its anchors; once hidden by the scrim, a drag, or
/// Escape, `on_dismiss_request` is called (stop calling `ModalBottomSheet` then).
#[composable]
pub fn ModalBottomSheet(
    on_dismiss_request: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_modal_bottom_sheet_state(false))] sheet_state: SheetState,
    #[default(crate::BottomSheetDefaults::SHEET_MAX_WIDTH)] sheet_max_width: Dp,
    #[default(crate::BottomSheetDefaults::expanded_shape())] shape: Shape,
    #[default(crate::BottomSheetDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::Dp(0.0))] tonal_elevation: Dp,
    #[default(crate::BottomSheetDefaults::scrim_color())] scrim_color: Color,
    #[default(crate::default_drag_handle())] drag_handle: Option<impl Fn() + 'static>,
    #[default(crate::BottomSheetDefaults::window_insets())] content_window_insets: PaddingValues,
    content: impl Fn(ColumnScope) + 'static,
) {
    let scope = remember_coroutine_scope();
    let density = compose_ui::LOCAL_DENSITY.current();
    {
        let state = sheet_state;
        LaunchedEffect(sheet_state, async move { state.show().await });
    }
    // Hides the sheet, then tells the caller.
    // The scrim hides the sheet (if `confirm_value_change` lets it), then tells the caller, as
    // Compose's `animateToDismiss`.
    let dismiss = {
        let (state, scope, on_dismiss) = (sheet_state, scope, on_dismiss_request.clone());
        compose_runtime::callback(move || {
            if !state.inner().is_some_and(|inner| inner.anchored.confirm(SheetValue::Hidden as u8))
            {
                return;
            }
            let (state, on_dismiss) = (state, on_dismiss.clone());
            scope.launch(async move {
                state.hide().await;
                if !state.is_visible() {
                    on_dismiss();
                }
            });
        })
    };
    // Back (Escape) takes an expanded sheet that can rest half way there, else hides it and
    // tells the caller, as Compose's sheet dialog's `onDismissRequest`.
    let back = {
        let (state, scope, on_dismiss) = (sheet_state, scope, on_dismiss_request.clone());
        compose_runtime::callback(move || {
            let (state, on_dismiss) = (state, on_dismiss.clone());
            if state.current_value() == SheetValue::Expanded && state.has_partially_expanded_state()
            {
                scope.launch(async move { state.partial_expand().await });
            } else {
                scope.launch(async move {
                    state.hide().await;
                    if !state.is_visible() {
                        on_dismiss();
                    }
                });
            }
        })
    };
    let settle = {
        let (state, scope, on_dismiss) = (sheet_state, scope, on_dismiss_request);
        let positional = density.to_px(POSITIONAL_THRESHOLD);
        let threshold = density.to_px(VELOCITY_THRESHOLD);
        Rc::new(move |velocity: f32| {
            let (state, on_dismiss) = (state, on_dismiss.clone());
            scope.launch(async move {
                if let Some(inner) = state.inner() {
                    inner.anchored.settle(velocity, &|_| positional, threshold).await;
                }
                if !state.is_visible() {
                    on_dismiss();
                }
            });
        }) as Rc<dyn Fn(f32)>
    };
    // The content's scrolls drag the sheet within its anchors first (up) or after it (down),
    // and its flings settle it, as Compose's `ConsumeSwipeWithinBottomSheetBoundsNestedScroll
    // Connection`.
    let connection = {
        let (state, settle) = (sheet_state, settle.clone());
        compose_runtime::remember_keyed(sheet_state, move || {
            Rc::new(SheetConnection { state, settle: std::cell::RefCell::new(settle) })
        })
    };
    *connection.settle.borrow_mut() = settle.clone();
    let connection: Rc<dyn NestedScrollConnection> = connection;
    let drag = {
        let state = sheet_state;
        compose_runtime::remember_keyed(sheet_state, move || {
            DraggableState::new(move |delta| {
                if let Some(inner) = state.inner() {
                    inner.anchored.drag_by(delta);
                }
            })
        })
    };
    let policy = {
        let state = sheet_state;
        compose_runtime::remember_keyed((sheet_state, sheet_max_width), move || {
            Rc::new(SheetLayout { state, max_width: sheet_max_width }) as Rc<dyn MeasurePolicy>
        })
    };
    // The scrim fades in while the sheet is going anywhere but hidden.
    let scrim_alpha = compose_animation::animate_float_as_state(
        if sheet_state.target_value() != SheetValue::Hidden { 1.0 } else { 0.0 },
        AnimationSpec::from(tween(300)),
        Spring::DEFAULT_DISPLACEMENT_THRESHOLD,
        "ScrimAlpha",
        None,
    );
    let provider = remember(|| Rc::new(AtWindowTop) as Rc<dyn PopupPositionProvider>);
    let properties = PopupProperties {
        focusable: true,
        dismiss_on_back_press: true,
        dismiss_on_click_outside: false,
    };
    let sheet_modifier = modifier
        .draggable(
            drag,
            Orientation::Vertical,
            true,
            false,
            |_| {},
            move |velocity| settle(velocity),
        )
        .nested_scroll(connection)
        .semantics(|config| config.set_content_description("Bottom sheet"));
    compose_ui::PopupWithPositionProvider(
        provider,
        Some(back),
        properties,
        compose_runtime::callback(move || {
            let (dismiss, content, drag_handle) =
                (dismiss.clone(), content.clone(), drag_handle.clone());
            let sheet_modifier = sheet_modifier.clone();
            Layout(
                Modifier,
                policy.clone(),
                compose_runtime::callback(move || {
                    Box(
                        Modifier
                            .fill_max_size()
                            .draw_behind(move |scope| {
                                let alpha = scrim_color.alpha() as f32 / 255.0 * scrim_alpha.get();
                                scope.draw_rect(
                                    scope.bounds(),
                                    compose_ui::Paint::fill(scrim_color.with_alpha(alpha)),
                                );
                            })
                            .then_custom(crate::drawer::ScrimElement {
                                close: dismiss.clone(),
                                enabled: true,
                                description: "Close sheet",
                            }),
                        Alignment::TOP_START,
                        false,
                        compose_runtime::callback(|_| {}),
                    );
                    let (content, drag_handle) = (content.clone(), drag_handle.clone());
                    crate::components::Surface(
                        sheet_modifier.clone().fill_max_width(),
                        shape,
                        container_color,
                        content_color,
                        tonal_elevation,
                        Dp(0.0),
                        None,
                        compose_runtime::callback(move || {
                            let drag_handle = drag_handle.clone();
                            let content = content.clone();
                            Column(
                                Modifier.fill_max_width().padding(content_window_insets),
                                compose_foundation::layout::Arrangement::TOP,
                                Alignment::START,
                                compose_runtime::callback(move |column: ColumnScope| {
                                    if let Some(drag_handle) = drag_handle.clone() {
                                        Box(
                                            Modifier.align(column, Alignment::CENTER_HORIZONTALLY),
                                            Alignment::TOP_START,
                                            false,
                                            compose_runtime::callback(move |_| drag_handle()),
                                        );
                                    }
                                    content(column);
                                }),
                            );
                        }),
                    );
                }),
            );
        }),
    );
}

/// The scrim over the window, and the sheet at the bottom: as wide as allowed up to its
/// maximum, centered, its top at the state's offset; the anchors from its height.
struct SheetLayout {
    state: SheetState,
    max_width: Dp,
}

impl MeasurePolicy for SheetLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let [scrim, sheet] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let (width, height) = (constraints.max_width, constraints.max_height);
        let scrim = scope.measure(*scrim, Constraints::fixed(width, height));
        let sheet_width = width.min(scope.density().round_to_px(self.max_width));
        let sheet = scope.measure(*sheet, Constraints::new(0, sheet_width, 0, height));
        let full = height as f32;
        let sheet_height = sheet.height as f32;
        let Some(inner) = self.state.inner() else {
            scope.place(&scrim, 0, 0);
            return scope.layout(width, height);
        };
        let mut anchors = Vec::with_capacity(3);
        if !inner.skip_hidden_state {
            anchors.push((SheetValue::Hidden, full));
        }
        if !inner.skip_partially_expanded && sheet_height > full / 2.0 {
            anchors.push((SheetValue::PartiallyExpanded, full / 2.0));
        }
        if sheet_height != 0.0 {
            anchors.push((SheetValue::Expanded, (full - sheet_height).max(0.0)));
        }
        inner
            .anchored
            .update_anchors(anchors.into_iter().map(|(value, at)| (value as u8, at)).collect());
        // Read while measuring: a frame of the slide places the sheet again only.
        let offset = inner.anchored.offset();
        let top = if offset.is_nan() { full } else { offset };
        scope.place(&scrim, 0, 0);
        scope.place(&sheet, (width - sheet.width) / 2, top.round() as i32);
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "ModalBottomSheet"
    }
}
