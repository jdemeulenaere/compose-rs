//! `ModalNavigationDrawer`, as Compose's Material 3: a sheet of destinations sliding in from
//! the start over the content, behind a scrim, with `ModalDrawerSheet` and
//! `NavigationDrawerItem`.

use compose_runtime::{ObjectHandle, Owned};
use compose_ui::ClickableOptions;
use std::rc::Rc;

use compose_animation::{Animatable, tween};
use compose_foundation::gestures::{DraggableModifier, DraggableState, Orientation};
use compose_foundation::layout::{Box, Column, ColumnScope, Row, Spacer};
use compose_runtime::{
    ComposeArg, CompositionLocalProvider, OwnedState, composable, mutable_state_of, remember,
    remember_coroutine_scope,
};
use compose_ui::{
    Alignment, Color, Constraints, CornerSize, Dp, Layout, LayoutDirection, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, Role, Shape, SizeSpec, UnitExt,
};

use crate::components::Surface;
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

/// As Compose's drawer animation: a 256 ms tween.
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
    /// Shown: open, opening, closing or dragged (its scrim and sheet are composed).
    shown: OwnedState<bool>,
}

impl DrawerInner {
    /// At `value`: settled there, its sheet composed only while open.
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

    #[track_caller]
    pub fn new(initial_value: DrawerValue) -> Owned<DrawerState> {
        let open = initial_value == DrawerValue::Open;
        Owned::new(
            DrawerInner {
                open: Animatable::new(if open { 1.0 } else { 0.0 }),
                target: mutable_state_of(initial_value),
                current: mutable_state_of(initial_value),
                shown: mutable_state_of(open),
            },
            |object| DrawerState { object },
        )
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
        self.animate_to(DrawerValue::Open).await;
    }

    /// Closes it, animating, as `close()`.
    pub async fn close(&self) {
        self.animate_to(DrawerValue::Closed).await;
    }

    /// Opens or closes it at once, as `snapTo`.
    pub fn snap_to(&self, value: DrawerValue) {
        let open = value == DrawerValue::Open;
        let Some(inner) = self.inner() else { return };
        inner.open.snap_to(if open { 1.0 } else { 0.0 });
        inner.settle(value);
    }

    async fn animate_to(&self, value: DrawerValue) {
        let open = value == DrawerValue::Open;
        // Kept across the animation: it ends even if the drawer's owner went meanwhile.
        let Some(inner) = self.inner() else { return };
        inner.target.set(value);
        inner.shown.set(true);
        let result =
            inner.open.animate_to(if open { 1.0 } else { 0.0 }, tween(ANIMATION_MILLIS)).await;
        if result.end_reason == compose_animation::AnimationEndReason::Finished {
            inner.settle(value);
        }
    }

    /// Dragged by `delta` pixels of a sheet `width` wide (positive towards open).
    fn drag_by(&self, delta: f32, width: f32) {
        let Some(inner) = self.inner() else { return };
        let fraction = (self.fraction() + delta / width.max(1.0)).clamp(0.0, 1.0);
        inner.open.snap_to(fraction);
        if fraction > 0.0 && !inner.shown.get() {
            inner.shown.set(true);
        }
    }

    /// How far open it is, 0 to 1 (a state read).
    fn fraction(&self) -> f32 {
        self.inner().map_or(0.0, |inner| inner.open.value().clamp(0.0, 1.0))
    }
}

impl ComposeArg for DrawerState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl DrawerState {
    /// Saves whether it's open, as Compose's `DrawerState.Saver` (its current value).
    pub fn saver() -> impl compose_runtime::Saver<Owned<DrawerState>> {
        compose_runtime::saver(
            |state: &Owned<DrawerState>| {
                state.inner()?;
                Some(compose_runtime::SavedValue::Bool(state.current_value() == DrawerValue::Open))
            },
            |saved| match saved {
                compose_runtime::SavedValue::Bool(open) => Some(DrawerState::new(if *open {
                    DrawerValue::Open
                } else {
                    DrawerValue::Closed
                })),
                _ => None,
            },
        )
    }
}

/// A drawer state remembered at its call site, as `rememberDrawerState(initialValue)`: saved
/// across the host's recreation (a `rememberSaveable`, as Compose's).
#[composable(non_restartable)]
pub fn remember_drawer_state(initial_value: DrawerValue) -> DrawerState {
    compose_runtime::remember_saveable_with(DrawerState::saver(), || {
        DrawerState::new(initial_value)
    })
}

/// As Compose's `DrawerDefaults`.
pub struct DrawerDefaults;

impl DrawerDefaults {
    /// A modal drawer sheet's widest, as `MaximumDrawerWidth`.
    pub const MAXIMUM_DRAWER_WIDTH: Dp = Dp(360.0);
    /// Its tonal elevation, as `ModalDrawerElevation`.
    pub const MODAL_DRAWER_ELEVATION: Dp = Dp(1.0);

    /// Its shape: large corners at its end, as `DrawerDefaults.shape`.
    pub fn shape() -> Shape {
        let radius = CornerSize::Dp(Dp(16.0));
        let none = CornerSize::Dp(Dp(0.0));
        // Top start, top end, bottom end, bottom start.
        Shape::Rounded([none, radius, radius, none])
    }

    pub fn modal_container_color() -> Color {
        MaterialTheme::color_scheme().surface_container_low
    }

    /// The scrim: 32 % of the scheme's scrim color, as `DrawerDefaults.scrimColor`.
    pub fn scrim_color() -> Color {
        MaterialTheme::color_scheme().scrim.with_alpha(0.32)
    }
}

/// `content` with a drawer over it, as Compose's `ModalNavigationDrawer`, filling the space it's
/// given: while it's open (or sliding), `drawer_content` slides in from the start edge over a
/// scrim. With `gestures_enabled`, a click on the scrim closes it, and a drag across the drawer
/// opens or closes it (settling by how far it went, or by the release's speed). Back (Android's
/// back, Escape on desktop) closes it while it's open.
#[composable]
#[allow(non_snake_case)]
pub fn ModalNavigationDrawer(
    drawer_content: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_drawer_state(crate::DrawerValue::Closed))] drawer_state: DrawerState,
    #[default(true)] gestures_enabled: bool,
    #[default(crate::DrawerDefaults::scrim_color())] scrim_color: Color,
    content: impl Fn() + 'static,
) {
    let scope = remember_coroutine_scope();
    let shown = drawer_state.inner().is_some_and(|inner| inner.shown.get());
    let rtl = compose_ui::LOCAL_LAYOUT_DIRECTION.current() == LayoutDirection::Rtl;
    let density = compose_ui::LOCAL_DENSITY.current();
    // The width the sheet was last laid out at: a drag moves it by fractions of it.
    let width = remember(|| Rc::new(std::cell::Cell::new(DrawerDefaults::MAXIMUM_DRAWER_WIDTH.0)));
    let policy = {
        let (state, width) = (drawer_state, width.clone());
        compose_runtime::remember_keyed(drawer_state, || {
            Rc::new(DrawerLayout { state, width }) as Rc<dyn MeasurePolicy>
        })
    };
    // A drag across the drawer moves the sheet; let go, it settles open or closed, as
    // Compose's `anchoredDraggable` on the drawer's box.
    let drag = {
        let (dragged, width) = (drawer_state, width.clone());
        compose_runtime::remember_keyed(drawer_state, move || {
            DraggableState::new(move |delta| dragged.drag_by(delta, width.get()))
        })
    };
    let threshold = density.to_px(VELOCITY_THRESHOLD);
    let settle = {
        let state = drawer_state;
        move |velocity: f32| {
            scope.launch(async move {
                let open = if velocity > threshold {
                    true
                } else if velocity < -threshold {
                    false
                } else {
                    state.fraction() > 0.5
                };
                if open { state.open().await } else { state.close().await }
            });
        }
    };
    let modifier = modifier.fill_max_size().draggable(
        drag,
        Orientation::Horizontal,
        gestures_enabled,
        rtl,
        |_| {},
        settle,
    );
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
                // The scrim, its alpha following the drawer (read while drawing).
                let fraction = state;
                let close = compose_runtime::callback(move || {
                    scope.launch(async move { state.close().await });
                });
                Box(
                    Modifier
                        .fill_max_size()
                        .draw_behind(move |draw| {
                            let alpha = scrim_color.alpha() as f32 / 255.0 * fraction.fraction();
                            draw.draw_rect(
                                draw.bounds(),
                                compose_ui::Paint::fill(scrim_color.with_alpha(alpha)),
                            );
                        })
                        .then_custom(ScrimElement {
                            close,
                            enabled: gestures_enabled,
                            description: "Close navigation menu",
                        }),
                    Alignment::TOP_START,
                    false,
                    compose_runtime::callback(|_| {}),
                );
                // Back closes it while it's open, as Compose's `DrawerPredictiveBackHandler`:
                // registered after the content's handlers (so it goes before them) and before
                // the sheet's (which go before it).
                compose_ui::BackHandler(
                    state.is_open(),
                    compose_runtime::callback(move || {
                        scope.launch(async move { state.close().await });
                    }),
                );
                let drawer_content = drawer_content.clone();
                Box(
                    Modifier,
                    Alignment::TOP_START,
                    false,
                    compose_runtime::callback(move |_| drawer_content()),
                );
            }
        }),
    );
}

/// The scrim's input: a click closes the drawer (with gestures enabled), as Compose's scrim's
/// `detectTapGestures`; it takes the pointer from the content beneath, and is no focus stop.
/// Bottom sheets' scrims are the same, with their own `description`.
pub(crate) struct ScrimElement {
    pub close: compose_runtime::Callback,
    pub enabled: bool,
    pub description: &'static str,
}

pub(crate) struct ScrimNode {
    close: compose_runtime::Callback,
    enabled: bool,
    description: &'static str,
    pressed: Option<compose_ui::PointerId>,
}

impl compose_ui::ModifierNodeElement for ScrimElement {
    type Node = ScrimNode;
    fn create(&self) -> ScrimNode {
        ScrimNode {
            close: self.close.clone(),
            enabled: self.enabled,
            description: self.description,
            pressed: None,
        }
    }
    fn update(&self, node: &mut ScrimNode) {
        node.close = self.close.clone();
        node.enabled = self.enabled;
        node.description = self.description;
    }
    fn same(&self, other: &Self) -> bool {
        other.close.ptr_eq(&self.close)
            && other.enabled == self.enabled
            && other.description == self.description
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
    fn apply_semantics(&self, config: &mut compose_ui::SemanticsConfiguration) {
        config.set_content_description(self.description);
        if self.enabled {
            let close = self.close.clone();
            config.on_click(None, move || {
                close();
                true
            });
        }
    }
}

/// The content, then (while shown) the scrim over it, then the sheet at the start edge, slid
/// out by how far the drawer is closed.
struct DrawerLayout {
    state: DrawerState,
    /// The sheet's width, for drags.
    width: Rc<std::cell::Cell<f32>>,
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
        if let [scrim, sheet] = rest {
            let scrim = scope.measure(*scrim, Constraints::fixed(width, height));
            scope.place_relative(&scrim, 0, 0);
            let sheet = scope.measure(*sheet, Constraints::new(0, width, height, height));
            self.width.set(sheet.width.max(1) as f32);
            // Read while measuring: a frame of the slide places the sheet again only.
            let hidden = 1.0 - self.state.fraction();
            scope.place_relative(&sheet, -(sheet.width as f32 * hidden).round() as i32, 0);
        }
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
    #[default(crate::DrawerDefaults::shape())] drawer_shape: Shape,
    #[default(crate::DrawerDefaults::modal_container_color())] drawer_container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] drawer_content_color: Color,
    #[default(crate::DrawerDefaults::MODAL_DRAWER_ELEVATION)] drawer_tonal_elevation: Dp,
    content: impl Fn(ColumnScope) + 'static,
) {
    let content_color = drawer_content_color
        .or(MaterialTheme::color_scheme().content_color_for(drawer_container_color));
    Surface(
        modifier
            .size_in(SizeSpec::bounds(
                240.dp(),
                Dp::UNSPECIFIED,
                DrawerDefaults::MAXIMUM_DRAWER_WIDTH,
                Dp::UNSPECIFIED,
            ))
            .fill_max_height()
            .semantics(|config| config.set_content_description("Navigation menu")),
        drawer_shape,
        drawer_container_color,
        content_color,
        drawer_tonal_elevation,
        Dp(0.0),
        None,
        compose_runtime::callback(move || {
            Column(
                Modifier.fill_max_height(),
                compose_foundation::layout::Arrangement::TOP,
                Alignment::START,
                content.clone(),
            );
        }),
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
        .clip(pill)
        .clickable_with(
            ClickableOptions {
                role: Some(Role::Tab),
                indication: text_color.into(),
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
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(text_color), || {
                CompositionLocalProvider(
                    LOCAL_TEXT_STYLE.provides(typography.label_large.clone()),
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
            });
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
