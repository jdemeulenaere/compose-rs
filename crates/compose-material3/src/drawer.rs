//! `ModalNavigationDrawer`, as Compose's Material 3: a sheet of destinations sliding in from
//! the start over the content, behind a scrim, with `ModalDrawerSheet` and
//! `NavigationDrawerItem`.

use compose_runtime::{ObjectHandle, Owned};
use compose_ui::{ClickableOptions, PredictiveBackHandler};
use std::cell::RefCell;
use std::rc::Rc;

use compose_animation::{Animatable, AnimationSpec, Easing, spring, tween};
use compose_foundation::gestures::{DraggableModifier, Orientation, draggable_state};
use compose_foundation::layout::{
    Box, Column, ColumnScope, Row, Spacer, WindowInsets, WindowInsetsModifiers, WindowInsetsSides,
};
use compose_runtime::{
    ComposeArg, CompositionLocalProvider, OwnedState, composable, mutable_state_of, remember,
    remember_coroutine_scope,
};
use compose_ui::{
    Alignment, Color, Constraints, CornerSize, Dp, Layout, LayoutDirection, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, Role, Shape, SizeSpec, UnitExt,
};

use crate::components::Surface;
use crate::motion::MotionScheme;
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

/// Whether a drawer is open, as Compose's `DrawerValue`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DrawerValue {
    Closed,
    Open,
}

impl ComposeArg for DrawerValue {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As `AnchoredDraggableDefaultAnimationSpec`: a 256 ms tween.
const ANIMATION_MILLIS: u32 = 256;
/// A release faster than this (towards one side) settles there, as Compose's drawer's
/// `DrawerVelocityThreshold` (400 dp per second).
const VELOCITY_THRESHOLD: Dp = Dp(400.0);

struct DrawerInner {
    /// How far open it is: 0 closed, 1 open (its offset, as a fraction of its width).
    open: Animatable<f32>,
    /// Where it's going (or is), as `targetValue`.
    target: OwnedState<DrawerValue>,
    /// Where it last settled, as `currentValue`.
    current: OwnedState<DrawerValue>,
    /// Shown: open, opening, closing or dragged (its scrim is composed and its sheet placed).
    shown: OwnedState<bool>,
    /// Animating to a value, as `isAnimationRunning`.
    animating: OwnedState<bool>,
    /// The sheet's width as last laid out (NaN before), the distance from closed to open.
    width: std::cell::Cell<f32>,
    /// Whether it may settle at a value, as `confirmStateChange`.
    confirm_state_change: Box<dyn Fn(DrawerValue) -> bool>,
    /// What it opens and closes with, and what a drag let go settles with, as
    /// `openDrawerMotionSpec`, `closeDrawerMotionSpec` and `anchoredDraggableMotionSpec`: the
    /// drawer sets them from the theme; before it's composed, as Compose's, opening and closing
    /// snap and a settle takes `AnchoredDraggableDefaultAnimationSpec`.
    open_spec: RefCell<AnimationSpec>,
    close_spec: RefCell<AnimationSpec>,
    settle_spec: RefCell<AnimationSpec>,
}

impl DrawerInner {
    /// At `value`: settled there, its sheet placed only while open.
    fn settle(&self, value: DrawerValue) {
        self.target.set(value);
        self.current.set(value);
        self.shown.set(value == DrawerValue::Open);
    }
}

/// A drawer's state, as Compose's `DrawerState`: open or closed, animated by `open` and
/// `close`.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_drawer_state`] keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DrawerState {
    object: ObjectHandle<DrawerInner>,
}

impl DrawerState {
    /// Its object, or `None` once its owner went (D240): then it does nothing, its reads
    /// giving the defaults.
    #[track_caller]
    fn inner(&self) -> Option<Rc<DrawerInner>> {
        self.object.get()
    }

    /// As `DrawerState(initialValue, confirmStateChange)`: `confirm_state_change` can keep it
    /// from settling at a value (a drag let go, `open`, `close` or `snap_to`); the drawer's scrim
    /// asks it before closing.
    #[track_caller]
    pub fn new(
        initial_value: DrawerValue,
        confirm_state_change: impl Fn(DrawerValue) -> bool + 'static,
    ) -> Owned<DrawerState> {
        let open = initial_value == DrawerValue::Open;
        Owned::new(
            DrawerInner {
                open: Animatable::new(if open { 1.0 } else { 0.0 }),
                target: mutable_state_of(initial_value),
                current: mutable_state_of(initial_value),
                shown: mutable_state_of(open),
                animating: mutable_state_of(false),
                width: std::cell::Cell::new(f32::NAN),
                confirm_state_change: Box::new(confirm_state_change),
                open_spec: RefCell::new(compose_animation::snap(0).into()),
                close_spec: RefCell::new(compose_animation::snap(0).into()),
                settle_spec: RefCell::new(tween(ANIMATION_MILLIS).into()),
            },
            |object| DrawerState { object },
        )
    }

    /// Whether `confirm_state_change` lets it settle at `value`.
    pub(crate) fn confirm_state_change(&self, value: DrawerValue) -> bool {
        self.inner().is_some_and(|inner| (inner.confirm_state_change)(value))
    }

    /// Whether it's animating to a value (`open`, `close`, or settling after a drag), as
    /// `isAnimationRunning` (a state read).
    pub fn is_animation_running(&self) -> bool {
        self.inner().is_some_and(|inner| inner.animating.get())
    }

    /// The sheet's offset from its open position, in pixels: 0 open, minus its width closed
    /// (towards the start edge), NaN before it's laid out, as `currentOffset` (a state read).
    pub fn current_offset(&self) -> f32 {
        let Some(inner) = self.inner() else { return f32::NAN };
        (inner.open.value() - 1.0) * inner.width.get()
    }

    /// Where it is or is going, as `targetValue` (a state read).
    pub fn target_value(&self) -> DrawerValue {
        self.inner().map_or(DrawerValue::Closed, |inner| inner.target.get())
    }

    /// Where it last settled, as `currentValue` (a state read): closed while it opens, open
    /// while it closes.
    pub fn current_value(&self) -> DrawerValue {
        self.inner().map_or(DrawerValue::Closed, |inner| inner.current.get())
    }

    pub fn is_open(&self) -> bool {
        self.current_value() == DrawerValue::Open
    }

    pub fn is_closed(&self) -> bool {
        !self.is_open()
    }

    /// Opens it, animating, as `open()`.
    pub async fn open(&self) {
        let Some(spec) = self.inner().map(|inner| inner.open_spec.borrow().clone()) else { return };
        self.animate_to(DrawerValue::Open, spec).await;
    }

    /// Closes it, animating, as `close()`.
    pub async fn close(&self) {
        let Some(spec) = self.inner().map(|inner| inner.close_spec.borrow().clone()) else {
            return;
        };
        self.animate_to(DrawerValue::Closed, spec).await;
    }

    /// A drag let go, to `value`, as the anchored draggable's fling.
    async fn settle_to(&self, value: DrawerValue) {
        let Some(spec) = self.inner().map(|inner| inner.settle_spec.borrow().clone()) else {
            return;
        };
        self.animate_to(value, spec).await;
    }

    /// Sets what `open` and `close` animate with, as both drawers do from the motion scheme.
    fn set_motion(&self, motion: &MotionScheme) {
        if let Some(inner) = self.inner() {
            *inner.open_spec.borrow_mut() = motion.default_spatial_spec();
            *inner.close_spec.borrow_mut() = motion.fast_effects_spec();
        }
    }

    /// Sets what a drag let go settles with, as only the modal drawer does: the dismissible one
    /// keeps the tween.
    fn set_settle_motion(&self, motion: &MotionScheme) {
        if let Some(inner) = self.inner() {
            *inner.settle_spec.borrow_mut() = motion.default_spatial_spec();
        }
    }

    /// Opens or closes it at once, as `snapTo`: settled there if `confirm_state_change` lets
    /// it (else the sheet moves and the value stays, as Compose's).
    pub fn snap_to(&self, value: DrawerValue) {
        let open = value == DrawerValue::Open;
        let Some(inner) = self.inner() else { return };
        inner.open.snap_to(if open { 1.0 } else { 0.0 });
        if (inner.confirm_state_change)(value) {
            inner.settle(value);
        } else {
            inner.shown.set(true);
        }
    }

    async fn animate_to(&self, value: DrawerValue, spec: AnimationSpec) {
        let open = value == DrawerValue::Open;
        // Kept across the animation: it ends even if the drawer's owner went meanwhile.
        let Some(inner) = self.inner() else { return };
        inner.target.set(value);
        inner.shown.set(true);
        inner.animating.set(true);
        let result = inner.open.animate_to(if open { 1.0 } else { 0.0 }, spec).await;
        if result.end_reason == compose_animation::AnimationEndReason::Finished {
            inner.animating.set(false);
            // Settled there only if `confirm_state_change` lets it, as Compose's
            // `anchoredDrag(targetValue)`.
            if (inner.confirm_state_change)(value) {
                inner.settle(value);
            } else {
                inner.target.set(inner.current.get());
            }
        }
    }

    /// Dragged by `delta` pixels of a sheet `width` wide (positive towards open); its target the
    /// nearer value, as Compose's `targetValue` while dragged.
    fn drag_by(&self, delta: f32, width: f32) {
        let Some(inner) = self.inner() else { return };
        let fraction = (self.fraction() + delta / width.max(1.0)).clamp(0.0, 1.0);
        inner.open.snap_to(fraction);
        if fraction > 0.0 && !inner.shown.get() {
            inner.shown.set(true);
        }
        let nearer = if fraction > 0.5 { DrawerValue::Open } else { DrawerValue::Closed };
        if inner.target.get() != nearer {
            inner.target.set(nearer);
        }
    }

    /// The sheet's width as last laid out, or the widest a sheet gets before that.
    fn width(&self, density: compose_ui::Density) -> f32 {
        let width = self.inner().map_or(f32::NAN, |inner| inner.width.get());
        if width.is_nan() { density.to_px(DrawerDefaults::MAXIMUM_DRAWER_WIDTH) } else { width }
    }

    /// How far open it is (a state read): 0 to 1, and past 1 as an expressive spring overshoots.
    fn fraction(&self) -> f32 {
        self.inner().map_or(0.0, |inner| inner.open.value())
    }
}

impl ComposeArg for DrawerState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl DrawerState {
    /// Saves whether it's open, as Compose's `DrawerState.Saver(confirmStateChange)` (its
    /// current value), restoring a state with `confirm_state_change`.
    pub fn saver(
        confirm_state_change: impl Fn(DrawerValue) -> bool + 'static,
    ) -> impl compose_runtime::Saver<Owned<DrawerState>> {
        let confirm_state_change = Rc::new(confirm_state_change);
        compose_runtime::saver(
            |state: &Owned<DrawerState>| {
                state.inner()?;
                Some(compose_runtime::SavedValue::Bool(state.current_value() == DrawerValue::Open))
            },
            move |saved| match saved {
                compose_runtime::SavedValue::Bool(open) => {
                    let confirm_state_change = confirm_state_change.clone();
                    Some(DrawerState::new(
                        if *open { DrawerValue::Open } else { DrawerValue::Closed },
                        move |value| confirm_state_change(value),
                    ))
                }
                _ => None,
            },
        )
    }
}

/// A drawer state remembered at its call site, as `rememberDrawerState(initialValue,
/// confirmStateChange)`: saved across the host's recreation (a `rememberSaveable`, as
/// Compose's). `confirm_state_change` is the first composition's.
#[composable(non_restartable)]
pub fn remember_drawer_state(
    initial_value: DrawerValue,
    #[default(|_| true)] confirm_state_change: impl Fn(DrawerValue) -> bool + 'static,
) -> DrawerState {
    let restored = confirm_state_change.clone();
    #[allow(clippy::redundant_closure)] // A `Callback` derefs to the function; it isn't one.
    compose_runtime::remember_saveable_with(
        DrawerState::saver(move |value| restored(value)),
        move || DrawerState::new(initial_value, move |value| confirm_state_change(value)),
    )
}

/// As Compose's `DrawerDefaults`.
pub struct DrawerDefaults;

impl DrawerDefaults {
    /// As `windowInsets`: the system bars and a cutout, at the top, the bottom and the start.
    pub fn window_insets() -> WindowInsets {
        crate::scaffold::system_bars_for_visual_components()
            .only(WindowInsetsSides::VERTICAL + WindowInsetsSides::START)
    }

    /// A modal drawer sheet's widest, as `MaximumDrawerWidth`.
    pub const MAXIMUM_DRAWER_WIDTH: Dp = Dp(360.0);
    /// Its tonal elevation, as `ModalDrawerElevation`.
    pub const MODAL_DRAWER_ELEVATION: Dp = Dp(0.0);
    /// A dismissible one's, as `DismissibleDrawerElevation`.
    pub const DISMISSIBLE_DRAWER_ELEVATION: Dp = Dp(0.0);
    /// A permanent one's, as `PermanentDrawerElevation`.
    pub const PERMANENT_DRAWER_ELEVATION: Dp = Dp(0.0);

    /// Its shape: large corners at its end, as `DrawerDefaults.shape`.
    pub fn shape() -> Shape {
        let radius = CornerSize::Dp(Dp(16.0));
        let none = CornerSize::Dp(Dp(0.0));
        // Top start, top end, bottom end, bottom start.
        Shape::Rounded([none, radius, radius, none])
    }

    /// A dismissible and a permanent sheet's, as `standardContainerColor`.
    pub fn standard_container_color() -> Color {
        MaterialTheme::color_scheme().surface
    }

    pub fn modal_container_color() -> Color {
        MaterialTheme::color_scheme().surface_container_low
    }

    /// The scrim: 32 % of the scheme's scrim color, as `DrawerDefaults.scrimColor`.
    pub fn scrim_color() -> Color {
        MaterialTheme::color_scheme().scrim.with_alpha(0.32)
    }
}

/// A drag across the drawer moves the sheet (by fractions of its width); let go, it settles open or
/// closed, as Compose's `anchoredDraggable` on the drawer's box.
fn drawer_gestures(
    modifier: Modifier,
    state: DrawerState,
    gestures_enabled: bool,
    scope: compose_runtime::CoroutineScope,
) -> Modifier {
    let rtl = compose_ui::LOCAL_LAYOUT_DIRECTION.current() == LayoutDirection::Rtl;
    let density = compose_ui::LOCAL_DENSITY.current();
    let drag = compose_runtime::remember_keyed((state, density), move || {
        draggable_state(move |delta| state.drag_by(delta, state.width(density)))
    });
    let threshold = density.to_px(VELOCITY_THRESHOLD);
    let settle = move |velocity: f32| {
        scope.launch(async move {
            let open = if velocity > threshold {
                true
            } else if velocity < -threshold {
                false
            } else {
                state.fraction() > 0.5
            };
            let target = if open { DrawerValue::Open } else { DrawerValue::Closed };
            // A value `confirm_state_change` refuses goes back to the settled one, as Compose's
            // `settle`.
            let target =
                if state.confirm_state_change(target) { target } else { state.current_value() };
            state.settle_to(target).await;
        });
    };
    modifier.draggable(drag, Orientation::Horizontal, gestures_enabled, rtl, |_| {}, settle)
}

/// The drawers' specs from the scheme, and the focus going in when it opens, as Compose's
/// `LaunchedEffect(isOpen) { focusRequester.requestFocus() }`: what to put on its pane.
fn drawer_motion_and_focus(state: DrawerState) -> compose_ui::FocusRequester {
    let motion = MaterialTheme::motion_scheme();
    compose_runtime::SideEffect(move || state.set_motion(&motion));
    let focus_requester = remember(compose_ui::FocusRequester::new);
    let is_open = state.is_open();
    compose_runtime::LaunchedEffect(is_open, async move {
        if is_open {
            focus_requester.request_focus();
        }
    });
    focus_requester
}

/// `content` with a drawer over it, as Compose's `ModalNavigationDrawer`, filling the space it's
/// given: while it's open (or sliding), `drawer_content` slides in from the start edge over a
/// scrim. With `gestures_enabled`, a click on the scrim closes it, and a drag across the drawer
/// opens or closes it (settling by how far it went, or by the release's speed). Back closes it
/// only if `ModalDrawerSheet` is given its `drawer_state`, as in Compose.
#[composable]
#[allow(non_snake_case)]
pub fn ModalNavigationDrawer(
    drawer_content: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_drawer_state(crate::DrawerValue::Closed, crate::always_confirm()))]
    drawer_state: DrawerState,
    #[default(true)] gestures_enabled: bool,
    #[default(crate::DrawerDefaults::scrim_color())] scrim_color: Color,
    content: impl Fn() + 'static,
) {
    let scope = remember_coroutine_scope();
    let shown = drawer_state.inner().is_some_and(|inner| inner.shown.get());
    let policy = {
        let state = drawer_state;
        compose_runtime::remember_keyed(drawer_state, || {
            Rc::new(DrawerLayout { state }) as Rc<dyn MeasurePolicy>
        })
    };
    let focus_requester = drawer_motion_and_focus(drawer_state);
    {
        let motion = MaterialTheme::motion_scheme();
        compose_runtime::SideEffect(move || drawer_state.set_settle_motion(&motion));
    }
    let modifier = drawer_gestures(modifier.fill_max_size(), drawer_state, gestures_enabled, scope);
    let state = drawer_state;
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            // The content in a box of its own, as Compose's: whatever it emits is one node.
            let content = content.clone();
            Box(
                Modifier,
                Alignment::TOP_START,
                false,
                compose_runtime::callback(move |_| content()),
            );
            if shown {
                // The scrim, its alpha following the drawer (read while drawing); its click
                // closes the drawer if `confirm_state_change` lets it, as Compose's.
                let fraction = state;
                let close = compose_runtime::callback(move || {
                    if gestures_enabled && state.confirm_state_change(DrawerValue::Closed) {
                        scope.launch(async move { state.close().await });
                    }
                });
                Box(
                    Modifier
                        .fill_max_size()
                        .draw_behind(move |draw| {
                            // Not past open, however far a spring takes the sheet.
                            let alpha =
                                scrim_color.alpha() as f32 / 255.0 * fraction.fraction().min(1.0);
                            draw.draw_rect(
                                draw.bounds(),
                                compose_ui::Paint::fill(scrim_color.with_alpha(alpha)),
                            );
                        })
                        .then_custom(ScrimElement {
                            close,
                            enabled: state.is_open(),
                            description: Some("Close navigation menu".into()),
                            traversal_index: Some(1.0),
                        }),
                    Alignment::TOP_START,
                    false,
                    compose_runtime::callback(|_| {}),
                );
            }
            // The sheet's pane, composed and measured even while closed, as Compose's: its width
            // gives the closed offset. What's in it is placed only while it shows, so that
            // keyboard focus doesn't reach it off screen. Opened, its first focusable takes the
            // focus, as Compose's `LaunchedEffect(isOpen) { focusRequester.requestFocus() }`.
            let drawer_content = drawer_content.clone();
            // Its pane, dismissed while open, as Compose's: refused by `confirm_state_change`.
            let pane = Modifier.semantics(move |config| {
                config.set_pane_title(NAVIGATION_MENU);
                if state.is_open() {
                    config.dismiss(None, move || {
                        if state.confirm_state_change(DrawerValue::Closed) {
                            scope.launch(async move { state.close().await });
                        }
                        true
                    });
                }
            });
            let pane_policy = compose_runtime::remember_keyed(state, move || {
                Rc::new(DrawerPaneLayout { state }) as Rc<dyn MeasurePolicy>
            });
            Layout(
                pane.focus_requester(focus_requester),
                pane_policy,
                compose_runtime::callback(move || drawer_content()),
            );
        }),
    );
}

/// The layout around a drawer's content, as the `Layout` of Compose's `ModalNavigationDrawer`: as
/// big as what's in it, which it places only while the drawer shows.
struct DrawerPaneLayout {
    state: DrawerState,
}

impl MeasurePolicy for DrawerPaneLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let loose = Constraints::new(0, constraints.max_width, 0, constraints.max_height);
        let placeables: Vec<_> = measurables.iter().map(|it| scope.measure(*it, loose)).collect();
        let width = placeables.iter().map(|it| it.width).max().unwrap_or(0);
        let height = placeables.iter().map(|it| it.height).max().unwrap_or(0);
        // Read while measuring, as `DrawerLayout`'s offset.
        if self.state.fraction() != 0.0 {
            for placeable in &placeables {
                scope.place_relative(placeable, 0, 0);
            }
        }
        scope.layout(constraints.constrain_width(width), constraints.constrain_height(height))
    }

    fn debug_name(&self) -> &'static str {
        "ModalNavigationDrawerPane"
    }
}

/// `Strings.NavigationMenu`.
const NAVIGATION_MENU: &str = "Navigation menu";

/// The scrim's input: a click closes the drawer (with gestures enabled), as Compose's scrim's
/// `detectTapGestures`; it takes the pointer from the content beneath, and is no focus stop.
/// Bottom sheets' scrims are the same, with their own `description`.
pub(crate) struct ScrimElement {
    pub close: compose_runtime::Callback,
    pub enabled: bool,
    pub description: Option<compose_ui::SharedString>,
    /// Its place in the traversal, for `Scrim`'s (after the content, `traversalIndex = 1f`).
    pub traversal_index: Option<f32>,
}

pub(crate) struct ScrimNode {
    close: compose_runtime::Callback,
    enabled: bool,
    description: Option<compose_ui::SharedString>,
    traversal_index: Option<f32>,
    pressed: Option<compose_ui::PointerId>,
}

impl compose_ui::ModifierNodeElement for ScrimElement {
    type Node = ScrimNode;
    fn create(&self) -> ScrimNode {
        ScrimNode {
            close: self.close.clone(),
            enabled: self.enabled,
            description: self.description.clone(),
            traversal_index: self.traversal_index,
            pressed: None,
        }
    }
    fn update(&self, node: &mut ScrimNode) {
        node.close = self.close.clone();
        node.enabled = self.enabled;
        node.description = self.description.clone();
        node.traversal_index = self.traversal_index;
    }
    fn same(&self, other: &Self) -> bool {
        other.close.ptr_eq(&self.close)
            && other.enabled == self.enabled
            && other.description == self.description
            && other.traversal_index == self.traversal_index
    }
    fn name(&self) -> &'static str {
        "drawer_scrim"
    }
}

impl compose_ui::ModifierNode for ScrimNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn compose_ui::PointerInputModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn compose_ui::SemanticsModifierNode> {
        Some(self)
    }
}

impl compose_ui::PointerInputModifierNode for ScrimNode {
    fn on_pointer_event(
        &mut self,
        event: &mut compose_ui::PointerEvent,
        pass: compose_ui::PointerEventPass,
        _bounds: compose_ui::IntSize,
        context: &mut compose_ui::EventContext<'_>,
    ) {
        use compose_ui::PointerEventType;
        // A move a drag took (by the final pass) is no click.
        if pass == compose_ui::PointerEventPass::Final
            && event.kind == PointerEventType::Move
            && event.consumed
        {
            self.pressed = None;
            return;
        }
        if pass != compose_ui::PointerEventPass::Main {
            return;
        }
        match event.kind {
            PointerEventType::Press if !event.consumed => {
                // Taken: the content beneath gets nothing.
                event.consume();
                self.pressed = Some(event.pointer);
            }
            PointerEventType::Release if self.pressed == Some(event.pointer) => {
                self.pressed = None;
                if self.enabled && !event.consumed {
                    event.consume();
                    context.call(&self.close);
                }
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.pressed = None;
    }
}

impl compose_ui::SemanticsModifierNode for ScrimNode {
    /// None unless it can be clicked, as `Scrim`, whose semantics come with its `onClick`.
    fn apply_semantics(&self, config: &mut compose_ui::SemanticsConfiguration) {
        if self.enabled {
            if let Some(description) = &self.description {
                config.set_content_description(description.clone());
            }
            if let Some(index) = self.traversal_index {
                config.set_traversal_index(index);
            }
            let close = self.close.clone();
            config.on_click(None, move || {
                close();
                true
            });
        }
    }
}

/// The content, then (while shown) the scrim over it, then the sheet at the start edge, slid
/// out by how far the drawer is closed (placed only while shown).
struct DrawerLayout {
    state: DrawerState,
}

impl MeasurePolicy for DrawerLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let Some((content, rest)) = measurables.split_first() else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let content = scope.measure(*content, constraints);
        let width = constraints.constrain_width(content.width);
        let height = constraints.constrain_height(content.height);
        // Read while measuring: a frame of the slide places the sheet again only.
        scope.place_relative(&content, 0, 0);
        let (scrim, sheet) = match rest {
            [scrim, sheet] => (Some(scrim), sheet),
            [sheet] => (None, sheet),
            _ => return scope.layout(width, height),
        };
        if let Some(scrim) = scrim {
            let scrim = scope.measure(*scrim, Constraints::fixed(width, height));
            scope.place_relative(&scrim, 0, 0);
        }
        let sheet = scope.measure(*sheet, Constraints::new(0, width, height, height));
        if let Some(inner) = self.state.inner() {
            inner.width.set(sheet.width.max(1) as f32);
        }
        // Read while measuring: a frame of the slide places the sheet again only. Closed, off
        // the start, as Compose's offset: the pane is placed, and so in the semantics, what's
        // in it not (`DrawerPaneLayout`).
        let hidden = 1.0 - self.state.fraction();
        scope.place_relative(&sheet, -(sheet.width as f32 * hidden).round() as i32, 0);
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "ModalNavigationDrawer"
    }
}

/// A modal drawer's sheet, as Compose's `ModalDrawerSheet`: as tall as the drawer, up to 360 dp
/// wide, on the low surface container with large corners at its end; its content a column.
#[composable]
#[allow(non_snake_case)]
pub fn ModalDrawerSheet(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] drawer_state: Option<DrawerState>,
    #[default(crate::DrawerDefaults::shape())] drawer_shape: Shape,
    #[default(crate::DrawerDefaults::modal_container_color())] drawer_container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] drawer_content_color: Color,
    #[default(crate::DrawerDefaults::MODAL_DRAWER_ELEVATION)] drawer_tonal_elevation: Dp,
    #[default(crate::DrawerDefaults::window_insets())] window_insets: WindowInsets,
    content: impl Fn(ColumnScope) + 'static,
) {
    let layers = compose_runtime::key(drawer_state.is_some(), || {
        drawer_state.map(|state| SheetLayers { state, back: drawer_predictive_back_handler(state) })
    });
    drawer_sheet(
        layers,
        modifier,
        drawer_shape,
        drawer_container_color,
        drawer_content_color,
        drawer_tonal_elevation,
        window_insets,
        content,
    );
}

/// The sheets' body, as Compose's `DrawerSheet`.
/// As `PredictiveBackDrawerMaxScaleXDistanceGrow`, `...Shrink` and `...YDistance`.
const PREDICTIVE_BACK_MAX_SCALE_X_GROW: Dp = Dp(12.0);
const PREDICTIVE_BACK_MAX_SCALE_X_SHRINK: Dp = Dp(24.0);
const PREDICTIVE_BACK_MAX_SCALE_Y: Dp = Dp(48.0);
/// As `PredictiveBack.transform`.
const PREDICTIVE_BACK_EASING: Easing = Easing::cubic_bezier(0.1, 0.1, 0.0, 1.0);

/// How far a back gesture has stretched a sheet, as Compose's `DrawerPredictiveBackState`.
struct DrawerPredictiveBackState {
    swipe_edge_matches_drawer: OwnedState<bool>,
    scale_x_distance: Animatable<f32>,
    scale_y_distance: OwnedState<f32>,
}

impl DrawerPredictiveBackState {
    fn update(
        &self,
        progress: f32,
        swipe_edge_left: bool,
        rtl: bool,
        density: compose_ui::Density,
    ) {
        let matches = swipe_edge_left != rtl;
        self.swipe_edge_matches_drawer.set(matches);
        let max_x = if matches {
            PREDICTIVE_BACK_MAX_SCALE_X_GROW
        } else {
            PREDICTIVE_BACK_MAX_SCALE_X_SHRINK
        };
        self.scale_x_distance.snap_to(density.to_px(max_x) * progress);
        self.scale_y_distance.set(density.to_px(PREDICTIVE_BACK_MAX_SCALE_Y) * progress);
    }

    fn clear(&self) {
        self.swipe_edge_matches_drawer.set(true);
        self.scale_x_distance.snap_to(0.0);
        self.scale_y_distance.set(0.0);
    }

    /// The container's scales for a sheet of `size`: toward the far edge from the drawer's, and
    /// less tall, as `calculatePredictiveBackScaleX` and `...Y`.
    fn scales(&self, size: compose_ui::Size) -> (f32, f32) {
        let x = if size.width == 0.0 {
            1.0
        } else {
            let direction = if self.swipe_edge_matches_drawer.get() { 1.0 } else { -1.0 };
            1.0 + self.scale_x_distance.value() * direction / size.width
        };
        let y =
            if size.height == 0.0 { 1.0 } else { 1.0 - self.scale_y_distance.get() / size.height };
        (x, y)
    }
}

/// Takes back while `state`'s target is open, stretching the sheet by the gesture and closing the
/// drawer at its end, as Compose's `DrawerPredictiveBackHandler`.
fn drawer_predictive_back_handler(state: DrawerState) -> Rc<DrawerPredictiveBackState> {
    let back = remember(|| {
        Rc::new(DrawerPredictiveBackState {
            swipe_edge_matches_drawer: mutable_state_of(true),
            scale_x_distance: Animatable::new(0.0),
            scale_y_distance: mutable_state_of(0.0),
        })
    });
    let scope = remember_coroutine_scope();
    let rtl = compose_ui::LOCAL_LAYOUT_DIRECTION.current() == LayoutDirection::Rtl;
    let density = compose_ui::LOCAL_DENSITY.current();
    {
        let back = back.clone();
        PredictiveBackHandler!(
            enabled = state.target_value() == DrawerValue::Open,
            on_back = move |mut progress: compose_ui::BackProgress| {
                let back = back.clone();
                async move {
                    loop {
                        match progress.next().await {
                            Ok(Some(event)) => back.update(
                                PREDICTIVE_BACK_EASING.transform(event.progress),
                                event.swipe_edge == compose_ui::BackEventCompat::EDGE_LEFT,
                                rtl,
                                density,
                            ),
                            Ok(None) => break,
                            // Compose's `catch` clears, and its `close()` in `finally` throws at
                            // once in the cancelled coroutine: it stays open.
                            Err(compose_ui::BackCancelled) => {
                                back.clear();
                                return;
                            }
                        }
                    }
                    // Stretched from the drawer's edge, it un-stretches, so that it hides whole
                    // as it closes.
                    if back.swipe_edge_matches_drawer.get() {
                        let back = back.clone();
                        scope.launch(async move {
                            back.scale_x_distance.animate_to(0.0, spring()).await;
                            back.clear();
                        });
                    }
                    state.close().await;
                }
            }
        );
    }
    let closed = !state.is_open() && state.target_value() == DrawerValue::Closed;
    {
        let back = back.clone();
        compose_runtime::LaunchedEffect(closed, async move {
            if closed {
                back.clear();
            }
        });
    }
    back
}

fn origin(pivot_fraction_x: f32, pivot_fraction_y: f32) -> compose_ui::TransformOrigin {
    compose_ui::TransformOrigin { pivot_fraction_x, pivot_fraction_y }
}

/// A sheet's stretch and its overshoot: on the container, `container`, and on the content inside
/// it, `content`, the content keeping its proportions, as Compose's `horizontalScaleUp`,
/// `horizontalScaleDown`, `predictiveBackDrawerContainer` and `predictiveBackDrawerChild`.
struct SheetLayers {
    state: DrawerState,
    back: Rc<DrawerPredictiveBackState>,
}

impl SheetLayers {
    fn overshoot_scale(state: DrawerState, density: compose_ui::Density) -> f32 {
        let offset = state.current_offset();
        if offset > 0.0 {
            1.0 + offset / density.to_px(DrawerDefaults::MAXIMUM_DRAWER_WIDTH)
        } else {
            1.0
        }
    }

    fn container(&self, modifier: Modifier, rtl: bool, density: compose_ui::Density) -> Modifier {
        let (state, back) = (self.state, self.back.clone());
        modifier
            .graphics_layer(move |layer| {
                layer.scale_x = Self::overshoot_scale(state, density);
                layer.transform_origin = origin(if rtl { 0.0 } else { 1.0 }, 0.5);
            })
            .graphics_layer(move |layer| {
                let (x, y) = back.scales(layer.size);
                layer.scale_x = x;
                layer.scale_y = y;
                layer.transform_origin = origin(if rtl { 1.0 } else { 0.0 }, 0.5);
            })
    }

    fn content(&self, modifier: Modifier, rtl: bool, density: compose_ui::Density) -> Modifier {
        let (state, back) = (self.state, self.back.clone());
        modifier
            .graphics_layer(move |layer| {
                layer.scale_x = 1.0 / Self::overshoot_scale(state, density);
                layer.transform_origin = origin(if rtl { 0.0 } else { 1.0 }, 0.0);
            })
            .graphics_layer(move |layer| {
                let (x, y) = back.scales(layer.size);
                layer.scale_x = if x != 0.0 { y / x } else { 1.0 };
                layer.transform_origin = origin(if rtl { 0.0 } else { 1.0 }, 0.0);
            })
    }
}

#[allow(clippy::too_many_arguments)]
fn drawer_sheet(
    layers: Option<SheetLayers>,
    modifier: Modifier,
    drawer_shape: Shape,
    drawer_container_color: Color,
    drawer_content_color: Color,
    drawer_tonal_elevation: Dp,
    window_insets: WindowInsets,
    content: compose_runtime::Callback<dyn Fn(ColumnScope)>,
) {
    let content_color = drawer_content_color
        .or(MaterialTheme::color_scheme().content_color_for(drawer_container_color));
    let rtl = compose_ui::LOCAL_LAYOUT_DIRECTION.current() == LayoutDirection::Rtl;
    let density = compose_ui::LOCAL_DENSITY.current();
    let sized = modifier.size_in(SizeSpec::bounds(
        240.dp(),
        Dp::UNSPECIFIED,
        DrawerDefaults::MAXIMUM_DRAWER_WIDTH,
        Dp::UNSPECIFIED,
    ));
    let container = match &layers {
        Some(layers) => layers.container(sized, rtl, density),
        None => sized,
    };
    Surface(
        container.fill_max_height(),
        drawer_shape,
        drawer_container_color,
        content_color,
        drawer_tonal_elevation,
        Dp(0.0),
        None,
        compose_runtime::callback(move || {
            // Its content inside the insets, as Compose's `windowInsetsPadding(windowInsets)`.
            let filled = Modifier.fill_max_height();
            let inside = match &layers {
                Some(layers) => layers.content(filled, rtl, density),
                None => filled,
            };
            Column(
                inside.window_insets_padding(window_insets.clone()),
                compose_foundation::layout::Arrangement::TOP,
                Alignment::START,
                content.clone(),
            );
        }),
    );
}

/// `content` beside a drawer that pushes it aside as it comes, as Compose's
/// `DismissibleNavigationDrawer`. A drag across it opens or closes it with `gestures_enabled`, Escape
/// closes it while it's open, and it has a pane title and a dismiss action. `DismissibleDrawerSheet`
/// with its `drawer_state` takes back.
#[composable]
#[allow(non_snake_case)]
pub fn DismissibleNavigationDrawer(
    drawer_content: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_drawer_state(crate::DrawerValue::Closed, crate::always_confirm()))]
    drawer_state: DrawerState,
    #[default(true)] gestures_enabled: bool,
    content: impl Fn() + 'static,
) {
    let state = drawer_state;
    let scope = remember_coroutine_scope();
    let focus_requester = drawer_motion_and_focus(state);
    let policy = compose_runtime::remember_keyed(state, || {
        Rc::new(DismissibleDrawerLayout { state }) as Rc<dyn MeasurePolicy>
    });
    let modifier = drawer_gestures(modifier, state, gestures_enabled, scope);
    Box(
        modifier,
        Alignment::TOP_START,
        false,
        compose_runtime::callback(move |_| {
            let (drawer_content, content, policy) =
                (drawer_content.clone(), content.clone(), policy.clone());
            Layout(
                Modifier,
                policy,
                compose_runtime::callback(move || {
                    let pane = Modifier
                        .semantics(move |config| {
                            config.set_pane_title(NAVIGATION_MENU);
                            if state.is_open() {
                                config.dismiss(None, move || {
                                    if state.confirm_state_change(DrawerValue::Closed) {
                                        scope.launch(async move { state.close().await });
                                    }
                                    true
                                });
                            }
                        })
                        // Escape closes it, as Compose's.
                        .on_key_event(move |event: compose_ui::KeyEvent| {
                            if state.is_open()
                                && event.kind == compose_ui::KeyEventType::KeyUp
                                && event.key == compose_ui::Key::Escape
                            {
                                scope.launch(async move { state.close().await });
                                return true;
                            }
                            false
                        })
                        .focus_requester(focus_requester);
                    let (drawer_content, content) = (drawer_content.clone(), content.clone());
                    Box(
                        pane,
                        Alignment::TOP_START,
                        false,
                        compose_runtime::callback(move |_| drawer_content()),
                    );
                    Box(
                        Modifier,
                        Alignment::TOP_START,
                        false,
                        compose_runtime::callback(move |_| content()),
                    );
                }),
            );
        }),
    );
}

/// As the `Layout` of Compose's `DismissibleNavigationDrawer`: the content's size, the content after
/// what shows of the sheet, the sheet placed only while it shows, so that keyboard focus doesn't
/// reach it off screen.
struct DismissibleDrawerLayout {
    state: DrawerState,
}

impl MeasurePolicy for DismissibleDrawerLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let sheet = scope.measure(measurables[0], constraints);
        let content = scope.measure(measurables[1], constraints);
        if let Some(inner) = self.state.inner() {
            inner.width.set(sheet.width.max(1) as f32);
        }
        // Read while measuring: a frame of the slide places them again only.
        let offset = -(sheet.width as f32 * (1.0 - self.state.fraction())).round() as i32;
        let content_x = sheet.width + offset;
        scope.place_relative(&content, content_x, 0);
        if content_x != 0 {
            scope.place_relative(&sheet, offset, 0);
        }
        scope.layout(content.width, content.height)
    }

    fn debug_name(&self) -> &'static str {
        "DismissibleNavigationDrawer"
    }
}

/// A dismissible drawer's sheet, as Compose's `DismissibleDrawerSheet`: as `ModalDrawerSheet`, square
/// on the surface color. With `drawer_state`, back closes the drawer, as the overload taking it
/// (without its predictive animation, feature `predictive-back`).
#[composable]
#[allow(non_snake_case)]
pub fn DismissibleDrawerSheet(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] drawer_state: Option<DrawerState>,
    #[default(crate::RECTANGLE_SHAPE)] drawer_shape: Shape,
    #[default(crate::DrawerDefaults::standard_container_color())] drawer_container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] drawer_content_color: Color,
    #[default(crate::DrawerDefaults::DISMISSIBLE_DRAWER_ELEVATION)] drawer_tonal_elevation: Dp,
    #[default(crate::DrawerDefaults::window_insets())] window_insets: WindowInsets,
    content: impl Fn(ColumnScope) + 'static,
) {
    let layers = compose_runtime::key(drawer_state.is_some(), || {
        drawer_state.map(|state| SheetLayers { state, back: drawer_predictive_back_handler(state) })
    });
    drawer_sheet(
        layers,
        modifier,
        drawer_shape,
        drawer_container_color,
        drawer_content_color,
        drawer_tonal_elevation,
        window_insets,
        content,
    );
}

/// `content` beside a drawer always there, as Compose's `PermanentNavigationDrawer`, filling the
/// space it's given.
#[composable]
#[allow(non_snake_case)]
pub fn PermanentNavigationDrawer(
    drawer_content: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    content: impl Fn() + 'static,
) {
    compose_foundation::layout::Row(
        modifier.fill_max_size(),
        compose_foundation::layout::Arrangement::START,
        Alignment::TOP,
        compose_runtime::callback(move |_| {
            drawer_content();
            let content = content.clone();
            Box(
                Modifier,
                Alignment::TOP_START,
                false,
                compose_runtime::callback(move |_| content()),
            );
        }),
    );
}

/// A permanent drawer's sheet, as Compose's `PermanentDrawerSheet`: as `DismissibleDrawerSheet`, with
/// a pane title.
#[composable]
#[allow(non_snake_case)]
pub fn PermanentDrawerSheet(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::RECTANGLE_SHAPE)] drawer_shape: Shape,
    #[default(crate::DrawerDefaults::standard_container_color())] drawer_container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] drawer_content_color: Color,
    #[default(crate::DrawerDefaults::PERMANENT_DRAWER_ELEVATION)] drawer_tonal_elevation: Dp,
    #[default(crate::DrawerDefaults::window_insets())] window_insets: WindowInsets,
    content: impl Fn(ColumnScope) + 'static,
) {
    drawer_sheet(
        None,
        modifier.semantics(|config| config.set_pane_title(NAVIGATION_MENU)),
        drawer_shape,
        drawer_container_color,
        drawer_content_color,
        drawer_tonal_elevation,
        window_insets,
        content,
    );
}

/// The colors of a `NavigationDrawerItem`, as Compose's `NavigationDrawerItemColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NavigationDrawerItemColors {
    pub selected_container_color: Color,
    pub unselected_container_color: Color,
    pub selected_icon_color: Color,
    pub unselected_icon_color: Color,
    pub selected_text_color: Color,
    pub unselected_text_color: Color,
}

impl ComposeArg for NavigationDrawerItemColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `NavigationDrawerItemDefaults`.
pub struct NavigationDrawerItemDefaults;

impl NavigationDrawerItemDefaults {
    pub fn colors() -> NavigationDrawerItemColors {
        let scheme = MaterialTheme::color_scheme();
        NavigationDrawerItemColors {
            selected_container_color: scheme.secondary_container,
            unselected_container_color: Color::TRANSPARENT,
            selected_icon_color: scheme.on_secondary_container,
            unselected_icon_color: scheme.on_surface_variant,
            selected_text_color: scheme.on_secondary_container,
            unselected_text_color: scheme.on_surface_variant,
        }
    }

    /// The padding around an item in a sheet, as `ItemPadding` (12 dp at the sides).
    pub fn item_padding(modifier: Modifier) -> Modifier {
        modifier.padding((12.dp(), 0.dp()))
    }
}

/// A destination in a drawer, as Compose's `NavigationDrawerItem`: at least 56 dp high in a pill, its
/// icon (24 dp) and label (label large) 16 dp in and 12 dp apart, a badge at the end; the
/// selected one in the secondary container.
#[composable]
#[allow(non_snake_case)]
pub fn NavigationDrawerItem(
    label: impl Fn() + 'static,
    selected: bool,
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    #[default(None)] badge: Option<impl Fn() + 'static>,
    #[default(crate::NavigationDrawerItemColors::default_colors())]
    colors: NavigationDrawerItemColors,
) {
    let pill = Shape::Rounded([CornerSize::Percent(50.0); 4]);
    let (container, icon_color, text_color) = if selected {
        (colors.selected_container_color, colors.selected_icon_color, colors.selected_text_color)
    } else {
        (
            colors.unselected_container_color,
            colors.unselected_icon_color,
            colors.unselected_text_color,
        )
    };
    let typography = MaterialTheme::typography();
    let row_modifier = modifier
        .size_in(SizeSpec::bounds(Dp::UNSPECIFIED, 56.dp(), Dp::UNSPECIFIED, Dp::UNSPECIFIED))
        .fill_max_width()
        .background_with(container, pill.clone())
        .clip(pill.clone())
        .clickable_with(
            ClickableOptions {
                role: Some(Role::Tab),
                indication: crate::ripple::shaped_ripple(pill.clone()),
                ..ClickableOptions::DEFAULT
            },
            on_click,
        )
        .semantics(move |config| config.set_selected(selected))
        .padding((16.dp(), 0.dp(), 24.dp(), 0.dp()));
    let (label, icon, badge) = (label.clone(), icon.clone(), badge.clone());
    Row(
        row_modifier,
        compose_foundation::layout::Arrangement::START,
        Alignment::CENTER_VERTICALLY,
        compose_runtime::callback(move |row| {
            if let Some(icon) = icon.clone() {
                CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(icon_color), &*icon);
                Spacer(Modifier.width(12.dp()));
            }
            let label = label.clone();
            CompositionLocalProvider(
                (
                    LOCAL_CONTENT_COLOR.provides(text_color),
                    LOCAL_TEXT_STYLE.provides(typography.label_large.clone()),
                ),
                || {
                    let label = label.clone();
                    Box(
                        Modifier.weight(row, 1.0),
                        Alignment::CENTER_START,
                        false,
                        compose_runtime::callback(move |_| label()),
                    );
                },
            );
            if let Some(badge) = badge.clone() {
                Spacer(Modifier.width(12.dp()));
                CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(text_color), &*badge);
            }
        }),
    );
}

impl NavigationDrawerItemColors {
    /// From the theme: `NavigationDrawerItemDefaults::colors()`.
    pub fn default_colors() -> NavigationDrawerItemColors {
        NavigationDrawerItemDefaults::colors()
    }
}
