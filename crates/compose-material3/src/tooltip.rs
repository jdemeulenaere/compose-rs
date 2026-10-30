//! `TooltipBox`, `PlainTooltip`, `RichTooltip` and `Label`, as Compose's Material 3 (`Tooltip.kt`,
//! `Label.kt`, `internal/BasicTooltip.kt`): a tooltip shown by a long press, a mouse's hover, the
//! keyboard's focus or `TooltipState::show`, positioned around its anchor, with a caret.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;
use std::task::{Poll, Waker};
use std::time::Duration;

use compose_animation::{MutableTransitionState, remember_transition};
use compose_foundation::layout::{Box, Column, PaddingValues};
use compose_foundation::{MutatePriority, MutatorMutex};
use compose_runtime::{
    Callback, ComposeArg, CompositionLocalProvider, CoroutineScope, ObjectHandle, Owned,
    OwnedState, composable, mutable_state_of, remember, remember_coroutine_scope,
};
use compose_ui::{
    Alignment, Color, CornerRadius, Density, Dp, DpSize, EventContext, FocusState, IntOffset,
    IntSize, Key, KeyEvent, KeyEventType, LayoutCoordinates, LayoutDirection, LiveRegionMode,
    Matrix, Modifier, ModifierNode, ModifierNodeElement, MutableInteractionSource, Offset, Outline,
    Path, PathOperation, PointerEvent, PointerEventPass, PointerEventType,
    PointerInputModifierNode, PointerType, PopupPositionProvider, PopupProperties, Rect, RoundRect,
    Shape, Size, SizeSpec,
};

use crate::components::Surface;
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

const SPACING_BETWEEN_TOOLTIP_AND_ANCHOR: Dp = Dp(4.0);
const TOOLTIP_MIN_HEIGHT: Dp = Dp(24.0);
const TOOLTIP_MIN_WIDTH: Dp = Dp(40.0);
const PLAIN_TOOLTIP_VERTICAL_PADDING: Dp = Dp(4.0);
const PLAIN_TOOLTIP_HORIZONTAL_PADDING: Dp = Dp(8.0);
const RICH_TOOLTIP_HORIZONTAL_PADDING: Dp = Dp(16.0);
const HEIGHT_TO_SUBHEAD_FIRST_LINE: Dp = Dp(28.0);
const HEIGHT_FROM_SUBHEAD_TO_TEXT_FIRST_LINE: Dp = Dp(24.0);
const TEXT_BOTTOM_PADDING: Dp = Dp(16.0);
const ACTION_LABEL_MIN_HEIGHT: Dp = Dp(36.0);
const ACTION_LABEL_BOTTOM_PADDING: Dp = Dp(8.0);
/// As `BasicTooltipDefaults.TooltipDuration`.
const TOOLTIP_DURATION: Duration = Duration::from_millis(1500);

thread_local! {
    /// As `BasicTooltipDefaults.GlobalMutatorMutex`: one tooltip at a time.
    static GLOBAL_MUTATOR_MUTEX: MutatorMutex = MutatorMutex::default();
}

/// Where a tooltip goes around its anchor, as Compose's `TooltipAnchorPosition`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TooltipAnchorPosition {
    Above,
    Below,
    Left,
    Right,
    /// `Left`, or `Right` right to left.
    Start,
    /// `Right`, or `Left` right to left.
    End,
}

impl ComposeArg for TooltipAnchorPosition {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Places a tooltip by a [`TooltipAnchorPosition`], flipped if it collides with the window's edge
/// and kept in the window, as Compose's `TooltipPositionProviderImpl`. Made by
/// [`TooltipDefaults::remember_tooltip_position_provider`].
pub struct TooltipPositionProvider {
    positioning: TooltipAnchorPosition,
    spacing: i32,
    /// Where the window's whole size comes from.
    window_info: Option<compose_ui::WindowInfo>,
}

impl TooltipPositionProvider {
    /// One that goes by the window it's given.
    pub fn new(positioning: TooltipAnchorPosition, spacing: i32) -> TooltipPositionProvider {
        TooltipPositionProvider { positioning, spacing, window_info: None }
    }

    fn beside(&self, anchor: Rect, popup: IntSize, window: IntSize, left_first: bool) -> IntOffset {
        let left = anchor.left.round() as i32 - (popup.width + self.spacing);
        let right = anchor.right.round() as i32 + self.spacing;
        let x = if left_first {
            if left < 0 { right } else { left }
        } else if right + popup.width > window.width {
            left
        } else {
            right
        };
        let y = (anchor.top.round() as i32 + anchor.bottom.round() as i32 - popup.height) / 2;
        kept_in(x, y, popup, window)
    }

    fn over(&self, anchor: Rect, popup: IntSize, window: IntSize, above_first: bool) -> IntOffset {
        let (left, width) = (anchor.left.round() as i32, anchor.width().round() as i32);
        let x = left + (width - popup.width) / 2;
        let above = anchor.top.round() as i32 - popup.height - self.spacing;
        let below = anchor.bottom.round() as i32 + self.spacing;
        let y = if above_first {
            if above < 0 { below } else { above }
        } else if below + popup.height > window.height {
            above
        } else {
            below
        };
        kept_in(x, y, popup, window)
    }
}

fn kept_in(x: i32, y: i32, popup: IntSize, window: IntSize) -> IntOffset {
    IntOffset::new(
        x.clamp(0, (window.width - popup.width).max(0)),
        y.clamp(0, (window.height - popup.height).max(0)),
    )
}

impl PopupPositionProvider for TooltipPositionProvider {
    fn position(
        &self,
        anchor: Rect,
        window: IntSize,
        layout_direction: LayoutDirection,
        popup: IntSize,
    ) -> IntOffset {
        // The window's whole, as Compose's `windowContainerSize`, the rule for what compares with
        // an anchor: see `PopupPositionProvider`.
        let window = self.window_info.as_ref().map_or(window, |info| info.container_size());
        let ltr = layout_direction == LayoutDirection::Ltr;
        match self.positioning {
            TooltipAnchorPosition::Left => self.beside(anchor, popup, window, true),
            TooltipAnchorPosition::Right => self.beside(anchor, popup, window, false),
            TooltipAnchorPosition::Above => self.over(anchor, popup, window, true),
            TooltipAnchorPosition::Below => self.over(anchor, popup, window, false),
            TooltipAnchorPosition::Start => self.beside(anchor, popup, window, ltr),
            TooltipAnchorPosition::End => self.beside(anchor, popup, window, !ltr),
        }
    }
}

/// As Compose's `TooltipDefaults`.
pub struct TooltipDefaults;

impl TooltipDefaults {
    /// As `caretSize`.
    pub const CARET_SIZE: DpSize = DpSize::new(Dp(16.0), Dp(8.0));
    /// As `plainTooltipMaxWidth`.
    pub const PLAIN_TOOLTIP_MAX_WIDTH: Dp = Dp(200.0);
    /// As `richTooltipMaxWidth`.
    pub const RICH_TOOLTIP_MAX_WIDTH: Dp = Dp(320.0);

    /// The extra-small shape, as `plainTooltipContainerShape`.
    pub fn plain_tooltip_container_shape() -> Shape {
        MaterialTheme::shapes().extra_small.clone()
    }

    /// The inverse surface, as `plainTooltipContainerColor`.
    pub fn plain_tooltip_container_color() -> Color {
        MaterialTheme::color_scheme().inverse_surface
    }

    /// The inverse on-surface, as `plainTooltipContentColor`.
    pub fn plain_tooltip_content_color() -> Color {
        MaterialTheme::color_scheme().inverse_on_surface
    }

    /// The medium shape, as `richTooltipContainerShape`.
    pub fn rich_tooltip_container_shape() -> Shape {
        MaterialTheme::shapes().medium.clone()
    }

    /// As `richTooltipColors()`; change them with struct update (D180).
    pub fn rich_tooltip_colors() -> RichTooltipColors {
        let scheme = MaterialTheme::color_scheme();
        RichTooltipColors {
            container_color: scheme.surface_container,
            content_color: scheme.on_surface_variant,
            title_content_color: scheme.on_surface_variant,
            action_content_color: scheme.primary,
        }
    }

    /// The triangle of `CARET_SIZE`, as `caretShape()`.
    pub fn caret_shape() -> Shape {
        Self::caret_shape_with(Self::CARET_SIZE)
    }

    /// A triangle of `caret_size`, as `caretShape(caretSize)`.
    pub fn caret_shape_with(caret_size: DpSize) -> Shape {
        Shape::custom(DefaultTooltipCaretShape { caret_size })
    }

    /// The position of `positioning`, 4 dp from the anchor, as
    /// `rememberTooltipPositionProvider(positioning)`.
    pub fn remember_tooltip_position_provider(
        positioning: TooltipAnchorPosition,
    ) -> Rc<dyn PopupPositionProvider> {
        Self::remember_tooltip_position_provider_with(
            positioning,
            SPACING_BETWEEN_TOOLTIP_AND_ANCHOR,
        )
    }

    /// As `rememberTooltipPositionProvider(positioning, spacingBetweenTooltipAndAnchor)`.
    pub fn remember_tooltip_position_provider_with(
        positioning: TooltipAnchorPosition,
        spacing_between_tooltip_and_anchor: Dp,
    ) -> Rc<dyn PopupPositionProvider> {
        let density = compose_ui::LOCAL_DENSITY.current();
        let spacing = density.round_to_px(spacing_between_tooltip_and_anchor);
        compose_runtime::remember_keyed((positioning, spacing), || {
            Rc::new(TooltipPositionProvider {
                positioning,
                spacing,
                window_info: Some(compose_ui::LOCAL_WINDOW_INFO.current()),
            }) as Rc<dyn PopupPositionProvider>
        })
    }
}

/// A rich tooltip's colors, as Compose's `RichTooltipColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RichTooltipColors {
    pub container_color: Color,
    pub content_color: Color,
    pub title_content_color: Color,
    pub action_content_color: Color,
}

impl ComposeArg for RichTooltipColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A tooltip's triangle, pointing down from the origin, as Compose's `DefaultTooltipCaretShape`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DefaultTooltipCaretShape {
    pub caret_size: DpSize,
}

impl compose_ui::CustomShape for DefaultTooltipCaretShape {
    fn create_outline(&self, _: Size, _: LayoutDirection, density: &Density) -> Outline {
        let (width, height) =
            (density.to_px(self.caret_size.width), density.to_px(self.caret_size.height));
        let path = Path::new();
        path.move_to(0.0, 0.0);
        path.line_to(width / 2.0, 0.0);
        path.line_to(0.0, height);
        path.line_to(-width / 2.0, 0.0);
        path.close();
        Outline::generic(&path)
    }

    fn same(&self, other: &dyn compose_ui::CustomShape) -> bool {
        other.downcast_ref::<DefaultTooltipCaretShape>() == Some(self)
    }
}

fn add_outline(path: &Path, outline: Outline) {
    match outline {
        Outline::Rectangle(rect) => path.add_rect(rect),
        Outline::Rounded(rect, [top_left, top_right, bottom_right, bottom_left]) => {
            path.add_round_rect(RoundRect {
                top_left_corner_radius: CornerRadius::new(top_left, top_left),
                top_right_corner_radius: CornerRadius::new(top_right, top_right),
                bottom_right_corner_radius: CornerRadius::new(bottom_right, bottom_right),
                bottom_left_corner_radius: CornerRadius::new(bottom_left, bottom_left),
                ..RoundRect::new(rect, CornerRadius::new(0.0, 0.0))
            });
        }
        Outline::Generic(data, offset) => path.add_path(&Path::from_shared_data(data), offset),
    }
}

/// The tooltip's shape and its caret where `matrix` puts it, as Compose's `TooltipCaretShape`: a
/// read of `matrix`, so a move of the caret draws it again.
struct TooltipCaretShape {
    matrix: compose_runtime::MutableState<Matrix>,
    tooltip_shape: Shape,
    caret_shape: Shape,
}

impl compose_ui::CustomShape for TooltipCaretShape {
    fn create_outline(&self, size: Size, direction: LayoutDirection, density: &Density) -> Outline {
        let (tooltip, caret) = (Path::new(), Path::new());
        add_outline(&tooltip, self.tooltip_shape.outline_in(size, density, direction));
        add_outline(&caret, self.caret_shape.outline_in(size, density, direction));
        caret.transform(&self.matrix.get());
        Outline::generic(&Path::combine(PathOperation::Union, &tooltip, &caret))
    }

    fn same(&self, other: &dyn compose_ui::CustomShape) -> bool {
        std::ptr::addr_eq(self as *const Self, other as *const dyn compose_ui::CustomShape)
    }
}

/// Where a caret's middle goes along a tooltip of `tooltip_width`, as Compose's `caretX`.
#[doc(hidden)]
pub fn caret_x(tooltip_width: f32, screen_width_px: i32, anchor: Rect) -> f32 {
    let screen = screen_width_px as f32;
    let middle = (anchor.left + anchor.right) / 2.0;
    if tooltip_width >= screen {
        middle
    } else if middle - tooltip_width / 2.0 < 0.0 {
        middle + (tooltip_width - screen).max(-anchor.left)
    } else if middle + tooltip_width / 2.0 > screen {
        middle + (tooltip_width - anchor.right).min(0.0)
    } else {
        tooltip_width / 2.0
    }
}

/// Where the caret goes on a tooltip at `tooltip` of an anchor at `anchor`, as Compose's
/// `layoutCaret`: on the side facing it.
fn caret_matrix(
    tooltip: Rect,
    anchor: Rect,
    positioning: Option<TooltipAnchorPosition>,
    screen_width: i32,
) -> Matrix {
    let (width, height) = (tooltip.width(), tooltip.height());
    let is_below = tooltip.top > anchor.top;
    let is_to_the_right = tooltip.left > anchor.left;
    // Beside it whichever the side, so with the same caret.
    let beside = !matches!(
        positioning,
        None | Some(TooltipAnchorPosition::Above | TooltipAnchorPosition::Below)
    );
    let position = if beside {
        Offset::new(if is_to_the_right { 0.0 } else { width }, height / 2.0)
    } else {
        Offset::new(caret_x(width, screen_width, anchor), if is_below { 0.0 } else { height })
    };
    // Its point at the anchor: a turn to the tooltip's side, or flipped from pointing down.
    let turn = if beside {
        Matrix::rotation(if is_to_the_right { 90.0 } else { -90.0 })
    } else if is_below {
        Matrix::scaling(1.0, -1.0)
    } else {
        Matrix::new()
    };
    let mut matrix = turn;
    matrix.values[12] = position.x;
    matrix.values[13] = position.y;
    matrix
}

/// The modifier and shape of a tooltip with a caret: `PlainTooltip`'s and `RichTooltip`'s start.
fn with_caret(
    scope: Option<TooltipScope>,
    caret_shape: Option<Shape>,
    shape: Shape,
    modifier: Modifier,
) -> (Modifier, Shape) {
    let matrix = remember(|| mutable_state_of(Matrix::new()));
    let (Some(scope), Some(caret_shape)) = (scope, caret_shape) else {
        return (modifier, shape);
    };
    let window_info = compose_ui::LOCAL_WINDOW_INFO.current();
    let positioned = Modifier
        .on_globally_positioned(move |coordinates: LayoutCoordinates| {
            if let Some(anchor) = scope.obtain_anchor_bounds() {
                let bounds = |of: LayoutCoordinates| {
                    let (at, size) = (of.position_in_window(), of.size());
                    Rect::new(at.x, at.y, at.x + size.width as f32, at.y + size.height as f32)
                };
                let provider = scope.obtain_position_provider();
                let positioning = (&*provider as &dyn std::any::Any)
                    .downcast_ref::<TooltipPositionProvider>()
                    .map(|provider| provider.positioning);
                matrix.set(caret_matrix(
                    bounds(coordinates),
                    bounds(anchor),
                    positioning,
                    window_info.container_size().width,
                ));
            }
        })
        .then(modifier);
    let combined = compose_runtime::remember_keyed((shape.clone(), caret_shape.clone()), || {
        Shape::custom(TooltipCaretShape { matrix, tooltip_shape: shape, caret_shape })
    });
    (positioned, combined)
}

/// What a tooltip's content is given, as Compose's `TooltipScope`: `PlainTooltip`'s and
/// `RichTooltip`'s `scope` when they have a caret.
#[derive(Clone)]
pub struct TooltipScope {
    inner: Rc<ScopeInner>,
}

struct ScopeInner {
    anchor: OwnedState<Option<LayoutCoordinates>>,
    provider: Rc<dyn PopupPositionProvider>,
}

impl ComposeArg for TooltipScope {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl TooltipScope {
    /// Where the anchor is, as `obtainAnchorBounds` (a state read).
    pub fn obtain_anchor_bounds(&self) -> Option<LayoutCoordinates> {
        self.inner.anchor.get()
    }

    /// As `obtainPositionProvider`.
    pub fn obtain_position_provider(&self) -> Rc<dyn PopupPositionProvider> {
        self.inner.provider.clone()
    }
}

enum Visibility {
    /// `TooltipStateImpl`: with the transition, seen until it's faded out.
    Animated(MutableTransitionState<bool>),
    /// `BasicTooltipStateImpl`: a `Label`'s, gone at once.
    Plain(OwnedState<bool>),
    /// `LabelStateImpl`: a persistent `Label`'s, always.
    Always,
}

struct TooltipInner {
    visibility: Visibility,
    persistent: bool,
    mutator_mutex: MutatorMutex,
    /// The running `show`'s wait, as Compose's `job`.
    job: RefCell<Option<Rc<ShowJob>>>,
}

#[derive(Default)]
struct ShowJob {
    cancelled: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}

impl ShowJob {
    fn cancel(&self) {
        self.cancelled.set(true);
        if let Some(waker) = self.waker.borrow_mut().take() {
            waker.wake();
        }
    }

    fn poll(&self, context: &mut std::task::Context<'_>) -> Poll<()> {
        if self.cancelled.get() {
            return Poll::Ready(());
        }
        *self.waker.borrow_mut() = Some(context.waker().clone());
        Poll::Pending
    }
}

impl TooltipInner {
    fn set_visible(&self, visible: bool) {
        match &self.visibility {
            Visibility::Animated(transition) => transition.set_target_state(visible),
            Visibility::Plain(state) => state.set(visible),
            Visibility::Always => {}
        }
    }

    fn dismiss(&self) {
        self.set_visible(false);
        if self.persistent
            && matches!(self.visibility, Visibility::Animated(_))
            && let Some(job) = &*self.job.borrow()
        {
            job.cancel();
        }
    }
}

/// `show`'s `finally`, which a dropped task runs too.
struct ShowFinally {
    inner: Rc<TooltipInner>,
    job: Rc<ShowJob>,
    priority: MutatePriority,
}

impl Drop for ShowFinally {
    fn drop(&mut self) {
        // An interrupted mutation's block drops after the next has started (D850): that show's
        // is the state now, and a finally of this one mustn't hide what it showed.
        let current =
            self.inner.job.borrow().as_ref().is_some_and(|job| Rc::ptr_eq(job, &self.job));
        if !current {
            return;
        }
        match self.inner.visibility {
            Visibility::Animated(_) => {
                if self.priority != MutatePriority::PreventUserInput {
                    self.inner.dismiss();
                }
            }
            _ => self.inner.set_visible(false),
        }
    }
}

/// A tooltip's state, as Compose's `TooltipState`: whether it shows, and showing and dismissing
/// it. A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_tooltip_state`] keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TooltipState {
    object: ObjectHandle<TooltipInner>,
}

impl ComposeArg for TooltipState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl TooltipState {
    #[track_caller]
    fn inner(&self) -> Option<Rc<TooltipInner>> {
        self.object.get()
    }

    /// As `TooltipState(initialIsVisible, isPersistent, mutatorMutex)`: a persistent one stays
    /// until dismissed; the others leave after 1.5 s. Showing one interrupts another of the same
    /// `mutator_mutex`: see [`global_mutator_mutex`](Self::global_mutator_mutex).
    #[track_caller]
    pub fn new(
        initial_is_visible: bool,
        is_persistent: bool,
        mutator_mutex: MutatorMutex,
    ) -> Owned<TooltipState> {
        Self::of(
            Visibility::Animated(MutableTransitionState::new(initial_is_visible)),
            is_persistent,
            mutator_mutex,
        )
    }

    #[track_caller]
    fn of(
        visibility: Visibility,
        persistent: bool,
        mutator_mutex: MutatorMutex,
    ) -> Owned<TooltipState> {
        Owned::new(
            TooltipInner { visibility, persistent, mutator_mutex, job: RefCell::new(None) },
            |object| TooltipState { object },
        )
    }

    /// The mutex a state has by default, so that one tooltip shows at a time, as
    /// `BasicTooltipDefaults.GlobalMutatorMutex`.
    pub fn global_mutator_mutex() -> MutatorMutex {
        GLOBAL_MUTATOR_MUTEX.with(MutatorMutex::clone)
    }

    /// Whether it shows, or is yet to finish fading out, as `isVisible` (a state read).
    pub fn is_visible(&self) -> bool {
        self.inner().is_some_and(|inner| match &inner.visibility {
            Visibility::Animated(transition) => {
                transition.current_state() || transition.target_state()
            }
            Visibility::Plain(state) => state.get(),
            Visibility::Always => true,
        })
    }

    /// As `isPersistent`.
    pub fn is_persistent(&self) -> bool {
        self.inner().is_some_and(|inner| inner.persistent)
    }

    /// Shows it, as `show()`.
    pub async fn show(&self) {
        self.show_with(MutatePriority::Default).await;
    }

    /// Shows it, returning when it's gone or interrupted, as `show(mutatePriority)`: for 1.5 s,
    /// unless it's persistent or the user's (a hover).
    pub async fn show_with(&self, priority: MutatePriority) {
        self.show_tracked(priority).await;
    }

    /// `show_with`, saying if it's still this show at its end: not if a later one took over.
    async fn show_tracked(&self, priority: MutatePriority) -> bool {
        let Some(inner) = self.inner() else { return false };
        if matches!(inner.visibility, Visibility::Always) {
            return false;
        }
        let mutex = inner.mutator_mutex.clone();
        let started = Rc::new(ShowJob::default());
        let (job, kept) = (started.clone(), inner.clone());
        let block = async move {
            *inner.job.borrow_mut() = Some(job.clone());
            inner.set_visible(true);
            let waits_for_it = inner.persistent || priority == MutatePriority::UserInput;
            let _finally = ShowFinally { inner, job: job.clone(), priority };
            if waits_for_it {
                std::future::poll_fn(|context| job.poll(context)).await;
            } else {
                let mut timeout = std::pin::pin!(compose_runtime::delay(TOOLTIP_DURATION));
                std::future::poll_fn(|context| {
                    if timeout.as_mut().poll(context).is_ready() {
                        return Poll::Ready(());
                    }
                    job.poll(context)
                })
                .await;
            }
        };
        let _ = mutex.mutate(priority, block).await;
        let job = kept.job.borrow();
        job.as_ref().is_some_and(|job| Rc::ptr_eq(job, &started))
    }

    /// Hides it, as `dismiss()`.
    pub fn dismiss(&self) {
        if let Some(inner) = self.inner() {
            inner.dismiss();
        }
    }

    /// As `onDispose()`: the box of it left.
    fn on_dispose(&self) {
        if let Some(inner) = self.inner()
            && let Some(job) = &*inner.job.borrow()
        {
            job.cancel();
        }
    }
}

/// A tooltip state remembered at its call site, as `rememberTooltipState`.
#[composable(non_restartable)]
pub fn remember_tooltip_state(
    #[default(false)] initial_is_visible: bool,
    #[default(false)] is_persistent: bool,
    #[default(crate::TooltipState::global_mutator_mutex())] mutator_mutex: MutatorMutex,
) -> TooltipState {
    compose_runtime::remember_keyed((is_persistent, mutator_mutex.clone()), move || {
        TooltipState::new(initial_is_visible, is_persistent, mutator_mutex.clone())
    })
}

/// A press of the anchor, as `handleGestures`: a long press shows the tooltip, until it's lifted;
/// a mouse's hover does, until it leaves.
struct AnchorGestures {
    state: TooltipState,
}

#[derive(Default)]
struct LongPress {
    /// Past the timeout, not lifted yet.
    held: Cell<bool>,
    /// `show` came back while still held, so the release dismisses.
    show_done: Cell<bool>,
}

struct AnchorGesturesNode {
    state: TooltipState,
    /// Waiting for the timeout.
    waiting: Option<compose_runtime::Job>,
    fingers: u32,
    press: Rc<LongPress>,
}

impl AnchorGesturesNode {
    /// The press ended, whichever way. The `show` of one held long enough goes on to its time,
    /// as Compose's: only a wait for the timeout is cancelled.
    fn end_press(&mut self) {
        let waiting = self.waiting.take();
        if let Some(job) = waiting.filter(|_| !self.press.held.get()) {
            job.cancel();
        }
        self.fingers = 0;
        if self.press.held.replace(false) && self.press.show_done.get() {
            self.state.dismiss();
        }
    }
}

impl ModifierNodeElement for AnchorGestures {
    type Node = AnchorGesturesNode;
    fn create(&self) -> AnchorGesturesNode {
        AnchorGesturesNode { state: self.state, waiting: None, fingers: 0, press: Rc::default() }
    }
    fn update(&self, node: &mut AnchorGesturesNode) {
        node.state = self.state;
    }
    fn same(&self, other: &Self) -> bool {
        self.state == other.state
    }
    fn name(&self) -> &'static str {
        "tooltipGestures"
    }
}

impl ModifierNode for AnchorGesturesNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for AnchorGesturesNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        let state = self.state;
        let mouse = event.pointer_type == PointerType::Mouse;
        if pass == PointerEventPass::Main && mouse {
            match event.kind {
                PointerEventType::Enter => {
                    context.coroutine_scope().launch(async move {
                        state.show_with(MutatePriority::UserInput).await;
                    });
                }
                PointerEventType::Exit if !state.is_persistent() => state.dismiss(),
                _ => {}
            }
            return;
        }
        if mouse {
            return;
        }
        // A move something inside took (a scroll) ends the press, as `waitForUpOrCancellation`.
        if pass == PointerEventPass::Final {
            if event.kind == PointerEventType::Move && self.fingers > 0 && event.consumed {
                self.end_press();
            }
            return;
        }
        if pass != PointerEventPass::Initial {
            return;
        }
        match event.kind {
            PointerEventType::Press if self.fingers > 0 => self.fingers += 1,
            PointerEventType::Press if !event.consumed => {
                self.fingers = 1;
                // Its own: the `show` of the last may be running yet.
                self.press = Rc::default();
                let press = self.press.clone();
                let timeout = context.view_configuration().long_press_timeout_millis;
                self.waiting = Some(context.coroutine_scope().launch(async move {
                    compose_runtime::delay(Duration::from_millis(timeout)).await;
                    press.held.set(true);
                    let latest = state.show_tracked(MutatePriority::PreventUserInput).await;
                    // Another press's or a hover's show took over: it's theirs to end.
                    if latest {
                        if press.held.get() {
                            press.show_done.set(true);
                        } else if state.is_visible() {
                            state.dismiss();
                        }
                    }
                }));
            }
            PointerEventType::Release if self.fingers > 0 => {
                self.fingers -= 1;
                if self.fingers == 0 {
                    // The children's click isn't for a long press, as Compose's `consume`.
                    if self.press.held.get() {
                        event.consume();
                    }
                    let job = self.waiting.take_if(|_| !self.press.held.get());
                    if let Some(job) = job {
                        job.cancel();
                    }
                    self.waiting = None;
                    if self.press.held.replace(false) && self.press.show_done.get() {
                        state.dismiss();
                    }
                }
            }
            PointerEventType::Move
                if self.fingers > 0
                    && (event.consumed || context.is_out_of_touch_target(event, bounds)) =>
            {
                self.end_press();
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.end_press();
    }
}

/// The keyboard's part of the anchor, as `keyboardBehavior`: its focus shows the tooltip, Escape
/// hides it, and Tab goes to a tooltip with an action.
fn keyboard_behavior(
    modifier: Modifier,
    state: TooltipState,
    scope: CoroutineScope,
    has_action: bool,
    force_focusable: compose_runtime::MutableState<bool>,
) -> Modifier {
    let received_focus = remember(|| Rc::new(Cell::new(false)));
    let focus_received = received_focus.clone();
    modifier
        .on_focus_changed(move |focus: FocusState| {
            let received_focus = focus_received.clone();
            scope.launch(async move {
                if focus.is_focused() {
                    received_focus.set(true);
                    state.show_with(MutatePriority::PreventUserInput).await;
                }
                if received_focus.get() && state.is_visible() && !focus.is_focused() {
                    received_focus.set(false);
                    state.dismiss();
                }
            });
        })
        .on_preview_key_event(move |event: KeyEvent| {
            if !state.is_visible() {
                force_focusable.set(false);
                return false;
            }
            if has_action
                && event.key == Key::Tab
                && event.kind == KeyEventType::KeyDown
                && !event.modifiers.shift
            {
                force_focusable.set(true);
                return true;
            }
            if event.key == Key::Escape && event.kind == KeyEventType::KeyDown {
                received_focus.set(false);
                state.dismiss();
                return true;
            }
            false
        })
}

/// `BasicTooltipBox`: the popup while it shows, and the anchor with its input.
#[allow(clippy::too_many_arguments)]
fn basic_tooltip_box(
    position_provider: Rc<dyn PopupPositionProvider>,
    tooltip: Callback,
    state: TooltipState,
    modifier: Modifier,
    on_dismiss_request: Option<Callback>,
    focusable: bool,
    enable_user_input: bool,
    has_action: bool,
    content: Callback,
) {
    let scope = remember_coroutine_scope();
    let force_focusable = remember(|| mutable_state_of(false));
    let anchor_scope = remember_coroutine_scope();
    // With a service on, Compose also forces it for accessibility: the host doesn't say (D1048).
    let focusable = focusable || (has_action && force_focusable.get());
    Box(
        Modifier,
        Alignment::TOP_START,
        false,
        compose_runtime::callback(move |_| {
            if state.is_visible() {
                let on_dismiss = on_dismiss_request.clone();
                let dismiss = compose_runtime::callback(move || match &on_dismiss {
                    Some(on_dismiss) => on_dismiss(),
                    None => {
                        if state.is_visible() {
                            state.dismiss();
                            // The keyboard's focus isn't trapped once it's gone.
                            force_focusable.set(false);
                        }
                    }
                });
                let tooltip = tooltip.clone();
                compose_ui::PopupWithPositionProvider(
                    position_provider.clone(),
                    Some(dismiss),
                    PopupProperties { focusable, ..PopupProperties::default() },
                    compose_runtime::callback(move || {
                        let tooltip = tooltip.clone();
                        Box(
                            Modifier.semantics(|config| {
                                config.set_live_region(LiveRegionMode::Assertive);
                                config.set_pane_title("tooltip");
                            }),
                            Alignment::TOP_START,
                            false,
                            compose_runtime::callback(move |_| tooltip()),
                        );
                    }),
                );
            }
            let anchor = if enable_user_input {
                let semantics = remember_keyed_long_click(state, anchor_scope);
                keyboard_behavior(
                    modifier
                        .clone()
                        .then_custom(AnchorGestures { state })
                        .then_custom(crate::parent_semantics::ParentSemantics(semantics)),
                    state,
                    scope,
                    has_action,
                    force_focusable,
                )
            } else {
                force_focusable.set(false);
                modifier.clone()
            };
            let content = content.clone();
            Box(anchor, Alignment::TOP_START, false, compose_runtime::callback(move |_| content()));
        }),
    );
    compose_runtime::DisposableEffect(state, move |effect| {
        effect.on_dispose(move || state.on_dispose())
    });
}

fn remember_keyed_long_click(
    state: TooltipState,
    scope: CoroutineScope,
) -> crate::parent_semantics::Properties {
    compose_runtime::remember_keyed(state, move || {
        Rc::new(move |config: &mut compose_ui::SemanticsConfiguration| {
            config.on_long_click(Some("show tooltip".into()), move || {
                scope.launch(async move { state.show().await });
                true
            });
        }) as crate::parent_semantics::Properties
    })
}

/// `content` with a tooltip, as Compose's `TooltipBox`: `tooltip`, typically a `PlainTooltip` or
/// `RichTooltip`, in a popup placed by `position_provider`, while `state` is visible. With
/// `enable_user_input`, a long press, a mouse's hover and the anchor's focus show it. Without
/// `on_dismiss_request`, a press outside dismisses it.
#[composable]
#[allow(non_snake_case)]
pub fn TooltipBox(
    position_provider: Rc<dyn PopupPositionProvider>,
    tooltip: impl Fn(TooltipScope) + 'static,
    state: TooltipState,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] on_dismiss_request: Option<impl Fn() + 'static>,
    #[default(false)] focusable: bool,
    #[default(true)] enable_user_input: bool,
    #[default(false)] has_action: bool,
    content: impl Fn() + 'static,
) {
    let provider = position_provider.clone();
    let scope_inner = remember(|| Rc::new(ScopeInner { anchor: mutable_state_of(None), provider }));
    let tooltip_position = remember(|| mutable_state_of(None::<Offset>));
    let inner = state.inner();
    // The transition's at the box, as Compose's: it goes on while the popup's there.
    let scales = inner.as_ref().and_then(|inner| match &inner.visibility {
        Visibility::Animated(transition) => {
            let transition = remember_transition(transition, "tooltip transition");
            let motion = MaterialTheme::motion_scheme();
            let (scale_spec, alpha_spec) = (motion.fast_spatial_spec(), motion.fast_effects_spec());
            let scale = transition.animate_float(
                move |_| scale_spec.clone(),
                |visible| if *visible { 1.0 } else { 0.8 },
            );
            let alpha = transition.animate_float(
                move |_| alpha_spec.clone(),
                |visible| if *visible { 1.0 } else { 0.0 },
            );
            Some((scale, alpha))
        }
        _ => None,
    });
    let anchored = scope_inner.clone();
    let wrapped = compose_runtime::callback(move || {
        let (anchored, content) = (anchored.clone(), content.clone());
        Box(
            Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
                anchored.anchor.set(Some(coordinates));
            }),
            Alignment::TOP_START,
            false,
            compose_runtime::callback(move |_| content()),
        );
    });
    let scope = TooltipScope { inner: scope_inner.clone() };
    let wrapped_tooltip = compose_runtime::callback(move || {
        let (scope, tooltip) = (scope.clone(), tooltip.clone());
        let side = tooltip_side(scope.obtain_anchor_bounds(), tooltip_position.get());
        Box(
            Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
                tooltip_position.set(Some(coordinates.position_in_window()));
            }),
            Alignment::TOP_START,
            false,
            compose_runtime::callback(move |_| {
                let (scope, tooltip) = (scope.clone(), tooltip.clone());
                // Made anew on a change of side, as Compose's `key(tooltipSide)`.
                compose_runtime::key(side, move || {
                    let (scope, tooltip) = (scope.clone(), tooltip.clone());
                    let layer = Modifier.graphics_layer(move |layer| {
                        if let Some((scale, alpha)) = &scales {
                            let scale_now = scale.get();
                            layer.scale_x = scale_now;
                            layer.scale_y = scale_now;
                            layer.alpha = alpha.get();
                        }
                    });
                    Box(
                        layer,
                        Alignment::TOP_START,
                        false,
                        compose_runtime::callback(move |_| tooltip(scope.clone())),
                    );
                });
            }),
        );
    });
    basic_tooltip_box(
        position_provider,
        wrapped_tooltip,
        state,
        modifier,
        on_dismiss_request,
        focusable,
        enable_user_input,
        has_action,
        wrapped,
    );
}

/// Which side of the anchor it's on, as Compose's `tooltipSide`.
fn tooltip_side(anchor: Option<LayoutCoordinates>, tooltip: Option<Offset>) -> i32 {
    let (Some(anchor), Some(tooltip)) = (anchor, tooltip) else { return 0 };
    let anchor = anchor.position_in_window();
    match (tooltip.x <= anchor.x, tooltip.y < anchor.y) {
        (true, true) => 1,
        (true, false) => 3,
        (false, true) => 2,
        (false, false) => 4,
    }
}

/// A tooltip's look, as Compose's `PlainTooltip`: `content` in the small body text on the inverse
/// surface, 8 dp across and 4 dp down from its edges, 40 to `max_width` wide, at least 24 dp high.
/// With `caret_shape` and its `scope`, a caret at its anchor. Also what foundation's `TooltipArea`
/// shows.
#[composable]
#[allow(non_snake_case)]
pub fn PlainTooltip(
    #[default(None)] scope: Option<TooltipScope>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] caret_shape: Option<Shape>,
    #[default(crate::TooltipDefaults::PLAIN_TOOLTIP_MAX_WIDTH)] max_width: Dp,
    #[default(crate::TooltipDefaults::plain_tooltip_container_shape())] shape: Shape,
    #[default(crate::TooltipDefaults::plain_tooltip_content_color())] content_color: Color,
    #[default(crate::TooltipDefaults::plain_tooltip_container_color())] container_color: Color,
    #[default(crate::Dp(0.0))] tonal_elevation: Dp,
    #[default(crate::Dp(0.0))] shadow_elevation: Dp,
    content: impl Fn() + 'static,
) {
    let (modifier, shape) = with_caret(scope, caret_shape, shape, modifier);
    Surface(
        modifier,
        shape,
        container_color,
        Color::UNSPECIFIED,
        tonal_elevation,
        shadow_elevation,
        None,
        compose_runtime::callback(move || {
            let content = content.clone();
            Box(
                Modifier
                    .size_in(SizeSpec::bounds(
                        TOOLTIP_MIN_WIDTH,
                        TOOLTIP_MIN_HEIGHT,
                        max_width,
                        Dp::UNSPECIFIED,
                    ))
                    .padding((PLAIN_TOOLTIP_HORIZONTAL_PADDING, PLAIN_TOOLTIP_VERTICAL_PADDING)),
                Alignment::TOP_START,
                false,
                compose_runtime::callback(move |_| {
                    let content = content.clone();
                    let style = MaterialTheme::typography().body_small.clone();
                    CompositionLocalProvider(
                        (
                            LOCAL_CONTENT_COLOR.provides(content_color),
                            LOCAL_TEXT_STYLE.provides(style),
                        ),
                        move || content(),
                    );
                }),
            );
        }),
    );
}

/// The top padding that puts a first baseline `top` down, as `paddingFromBaseline(top)` with no
/// alignment lines here yet: the baseline's taken at 80 % of the style's line.
fn padding_to_baseline(top: Dp, style: &compose_ui::TextStyle) -> Dp {
    let density = compose_ui::LOCAL_DENSITY.current();
    let line = density.sp_to_px(style.line_height);
    Dp((density.to_px(top) - line * 0.8).max(0.0) / density.density)
}

/// A tooltip with a title and an action, as Compose's `RichTooltip`.
#[composable]
#[allow(non_snake_case)]
pub fn RichTooltip(
    #[default(None)] scope: Option<TooltipScope>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] title: Option<impl Fn() + 'static>,
    #[default(None)] action: Option<impl Fn() + 'static>,
    #[default(None)] caret_shape: Option<Shape>,
    #[default(crate::TooltipDefaults::RICH_TOOLTIP_MAX_WIDTH)] max_width: Dp,
    #[default(crate::TooltipDefaults::rich_tooltip_container_shape())] shape: Shape,
    #[default(crate::TooltipDefaults::rich_tooltip_colors())] colors: RichTooltipColors,
    #[default(crate::Dp(0.0))] tonal_elevation: Dp,
    #[default(crate::Dp(3.0))] shadow_elevation: Dp,
    text: impl Fn() + 'static,
) {
    let (modifier, shape) = with_caret(scope, caret_shape, shape, modifier);
    let has_title = title.is_some();
    let has_action = action.is_some();
    Surface(
        modifier.size_in(SizeSpec::bounds(
            TOOLTIP_MIN_WIDTH,
            TOOLTIP_MIN_HEIGHT,
            max_width,
            Dp::UNSPECIFIED,
        )),
        shape,
        colors.container_color,
        Color::UNSPECIFIED,
        tonal_elevation,
        shadow_elevation,
        None,
        compose_runtime::callback(move || {
            let (title, action, text) = (title.clone(), action.clone(), text.clone());
            Column(
                Modifier.padding((RICH_TOOLTIP_HORIZONTAL_PADDING, Dp(0.0))),
                compose_foundation::layout::Arrangement::TOP,
                Alignment::START,
                compose_runtime::callback(move |_| {
                    let typography = MaterialTheme::typography();
                    let (subhead, supporting, label) = (
                        typography.title_small.clone(),
                        typography.body_medium.clone(),
                        typography.label_large.clone(),
                    );
                    if let Some(title) = title.clone() {
                        let top = padding_to_baseline(HEIGHT_TO_SUBHEAD_FIRST_LINE, &subhead);
                        slot(
                            Modifier.padding(PaddingValues { top, ..PaddingValues::ZERO }),
                            colors.title_content_color,
                            subhead,
                            title,
                        );
                    }
                    let vertical = if !has_title && !has_action {
                        PaddingValues {
                            top: PLAIN_TOOLTIP_VERTICAL_PADDING,
                            bottom: PLAIN_TOOLTIP_VERTICAL_PADDING,
                            ..PaddingValues::ZERO
                        }
                    } else {
                        PaddingValues {
                            top: padding_to_baseline(
                                HEIGHT_FROM_SUBHEAD_TO_TEXT_FIRST_LINE,
                                &supporting,
                            ),
                            bottom: TEXT_BOTTOM_PADDING,
                            ..PaddingValues::ZERO
                        }
                    };
                    slot(
                        Modifier.padding(vertical),
                        colors.content_color,
                        supporting,
                        text.clone(),
                    );
                    if let Some(action) = action.clone() {
                        slot(
                            Modifier
                                .size_in(SizeSpec {
                                    min_height: ACTION_LABEL_MIN_HEIGHT,
                                    enforce_incoming: false,
                                    ..SizeSpec::bounds(
                                        Dp::UNSPECIFIED,
                                        Dp::UNSPECIFIED,
                                        Dp::UNSPECIFIED,
                                        Dp::UNSPECIFIED,
                                    )
                                })
                                .padding(PaddingValues {
                                    bottom: ACTION_LABEL_BOTTOM_PADDING,
                                    ..PaddingValues::ZERO
                                }),
                            colors.action_content_color,
                            label,
                            action,
                        );
                    }
                }),
            );
        }),
    );
}

/// `content` in a box, in its color and text style.
fn slot(modifier: Modifier, color: Color, style: compose_ui::TextStyle, content: Callback) {
    Box(
        modifier,
        Alignment::TOP_START,
        false,
        compose_runtime::callback(move |_| {
            let (content, style) = (content.clone(), style.clone());
            #[allow(clippy::redundant_closure)]
            // A `Callback` derefs to the function, not `Sized`.
            CompositionLocalProvider(
                (LOCAL_CONTENT_COLOR.provides(color), LOCAL_TEXT_STYLE.provides(style)),
                move || content(),
            );
        }),
    );
}

/// A label shown while `content` is interacted with, as Compose's `Label`: above it, shown by
/// the interactions of `interaction_source` (a press, a drag, a hover), or always if
/// `is_persistent`. The content's own `interaction_source` goes here too.
#[composable]
#[allow(non_snake_case)]
pub fn Label(
    label: impl Fn(TooltipScope) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    #[default(false)] is_persistent: bool,
    content: impl Fn() + 'static,
) {
    let source = compose_runtime::remember_keyed(interaction_source.clone(), move || {
        interaction_source.unwrap_or_default()
    });
    let provider =
        TooltipDefaults::remember_tooltip_position_provider(TooltipAnchorPosition::Above);
    let state = compose_runtime::remember_keyed(is_persistent, move || {
        if is_persistent {
            TooltipState::of(Visibility::Always, true, MutatorMutex::default())
        } else {
            TooltipState::of(
                Visibility::Plain(mutable_state_of(false)),
                true,
                MutatorMutex::default(),
            )
        }
    });
    let positioned = provider.clone();
    let scope_inner =
        remember(|| Rc::new(ScopeInner { anchor: mutable_state_of(None), provider: positioned }));
    let anchored = scope_inner.clone();
    let wrapped = compose_runtime::callback(move || {
        let (anchored, content) = (anchored.clone(), content.clone());
        Box(
            Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
                anchored.anchor.set(Some(coordinates));
            }),
            Alignment::TOP_START,
            false,
            compose_runtime::callback(move |_| content()),
        );
    });
    let scope = TooltipScope { inner: scope_inner };
    basic_tooltip_box(
        provider,
        compose_runtime::callback(move || label(scope.clone())),
        state,
        modifier,
        None,
        false,
        false,
        false,
        wrapped,
    );
    compose_runtime::key(is_persistent, || {
        if !is_persistent {
            let source = source.clone();
            let launcher = remember_coroutine_scope();
            compose_runtime::LaunchedEffect(source.clone(), async move {
                let mut interactions = source.interactions();
                let mut active: Vec<LabelInteraction> = Vec::new();
                let mut was_visible = false;
                loop {
                    use compose_ui::{
                        DragInteraction, HoverInteraction, Interaction, PressInteraction,
                    };
                    match interactions.next().await {
                        Interaction::Press(PressInteraction::Press(press)) => {
                            active.push(LabelInteraction::Press(press));
                        }
                        Interaction::Drag(DragInteraction::Start(start)) => {
                            active.push(LabelInteraction::Drag(start));
                        }
                        Interaction::Hover(HoverInteraction::Enter(enter)) => {
                            active.push(LabelInteraction::Hover(enter));
                        }
                        Interaction::Press(
                            PressInteraction::Release(press) | PressInteraction::Cancel(press),
                        ) => active.retain(|it| *it != LabelInteraction::Press(press)),
                        Interaction::Drag(
                            DragInteraction::Stop(start) | DragInteraction::Cancel(start),
                        ) => active.retain(|it| *it != LabelInteraction::Drag(start)),
                        Interaction::Hover(HoverInteraction::Exit(enter)) => {
                            active.retain(|it| *it != LabelInteraction::Hover(enter));
                        }
                        _ => {}
                    }
                    // Only on a change: the slider's thumb shrinking from under the pointer as a
                    // drag starts doesn't dismiss it.
                    let is_visible = !active.is_empty();
                    if is_visible != was_visible {
                        was_visible = is_visible;
                        if is_visible {
                            launcher.launch(async move {
                                state.show_with(MutatePriority::UserInput).await;
                            });
                        } else {
                            state.dismiss();
                        }
                    }
                }
            });
        }
    });
}

#[derive(Clone, Copy, PartialEq)]
enum LabelInteraction {
    Press(compose_ui::Press),
    Drag(compose_ui::DragStart),
    Hover(compose_ui::HoverEnter),
}
