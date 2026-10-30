//! `BottomSheetScaffold`, as Compose's: a screen's top bar, body and snackbars with a standard
//! bottom sheet over them, resting at its peek height.

use std::rc::Rc;

use compose_foundation::gestures::{
    AnchoredDraggableModifier, AnchoredDraggableOptions, DraggableAnchors, Orientation,
};
use compose_foundation::layout::{Box, Column, ColumnScope};
use compose_runtime::{CompositionLocalProvider, composable, remember_coroutine_scope};
use compose_ui::{
    Alignment, Color, Constraints, Dp, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier, NestedScrollConnection, PaddingValues, Shape, SizeSpec,
};

use crate::sheet::{FlingKey, SheetConnection, SheetState, SheetValue};
use crate::snackbar::SnackbarHostState;
use crate::theme::{LOCAL_CONTENT_COLOR, MaterialTheme};

/// A scaffold's state, as Compose's `BottomSheetScaffoldState`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BottomSheetScaffoldState {
    pub bottom_sheet_state: SheetState,
    pub snackbar_host_state: SnackbarHostState,
}

impl compose_runtime::ComposeArg for BottomSheetScaffoldState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The values a scaffold's sheet rests at by default: never hidden.
const SCAFFOLD_VALUES: [SheetValue; 2] = [SheetValue::PartiallyExpanded, SheetValue::Expanded];

/// A scaffold's state remembered here, as `rememberBottomSheetScaffoldState`: a sheet resting
/// partially expanded that can't hide, and a snackbar host, unless given.
#[composable(non_restartable)]
pub fn remember_bottom_sheet_scaffold_state(
    #[default(None)] bottom_sheet_state: Option<SheetState>,
    #[default(None)] snackbar_host_state: Option<SnackbarHostState>,
) -> BottomSheetScaffoldState {
    let default_sheet = crate::sheet::remember_bottom_sheet_state(
        SheetValue::PartiallyExpanded,
        &SCAFFOLD_VALUES,
        crate::always_confirm(),
    );
    let default_host = compose_runtime::remember(SnackbarHostState::new);
    BottomSheetScaffoldState {
        bottom_sheet_state: bottom_sheet_state.unwrap_or(default_sheet),
        snackbar_host_state: snackbar_host_state.unwrap_or(default_host),
    }
}

/// The default snackbar host of a scaffold, as `{ SnackbarHost(it) }`.
#[doc(hidden)]
pub fn default_scaffold_snackbar_host() -> compose_runtime::Callback<dyn Fn(SnackbarHostState)> {
    compose_runtime::callback(|state: SnackbarHostState| {
        crate::snackbar::SnackbarHost(
            state,
            Modifier,
            None::<compose_runtime::Callback<dyn Fn(crate::snackbar::SnackbarData)>>,
        );
    })
}

/// A standard bottom sheet in a screen, as Compose's `BottomSheetScaffold`: `top_bar` at the top,
/// `content` below it (given the sheet's peek height as padding), the sheet over them, resting
/// at `sheet_peek_height` and dragged (with `sheet_swipe_enabled`) up to its full height, and
/// `snackbar_host` above the sheet while it peeks, else at the bottom.
#[composable]
#[allow(clippy::too_many_arguments)]
pub fn BottomSheetScaffold(
    sheet_content: impl Fn(ColumnScope) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_bottom_sheet_scaffold_state(None, None))]
    scaffold_state: BottomSheetScaffoldState,
    #[default(crate::BottomSheetDefaults::SHEET_PEEK_HEIGHT)] sheet_peek_height: Dp,
    #[default(crate::BottomSheetDefaults::SHEET_MAX_WIDTH)] sheet_max_width: Dp,
    #[default(crate::BottomSheetDefaults::expanded_shape())] sheet_shape: Shape,
    #[default(crate::BottomSheetDefaults::container_color())] sheet_container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] sheet_content_color: Color,
    #[default(crate::Dp(0.0))] sheet_tonal_elevation: Dp,
    #[default(crate::BottomSheetDefaults::ELEVATION)] sheet_shadow_elevation: Dp,
    #[default(crate::default_drag_handle())] sheet_drag_handle: Option<impl Fn() + 'static>,
    #[default(true)] sheet_swipe_enabled: bool,
    #[default(None)] top_bar: Option<impl Fn() + 'static>,
    #[default(crate::default_scaffold_snackbar_host())] snackbar_host: impl Fn(SnackbarHostState)
    + 'static,
    #[default(crate::MaterialTheme::color_scheme().surface)] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    content: impl Fn(PaddingValues) + 'static,
) {
    let state = scaffold_state.bottom_sheet_state;
    let host_state = scaffold_state.snackbar_host_state;
    let content_color =
        content_color.or(crate::MaterialTheme::color_scheme().content_color_for(container_color));
    let policy = compose_runtime::remember_keyed((state, sheet_peek_height), move || {
        Rc::new(ScaffoldLayout { state, peek_height: sheet_peek_height }) as Rc<dyn MeasurePolicy>
    });
    Box(
        modifier.fill_max_size().background(container_color),
        Alignment::TOP_START,
        false,
        compose_runtime::callback(move |_| {
            let (sheet_content, sheet_shape) = (sheet_content.clone(), sheet_shape.clone());
            let (sheet_drag_handle, top_bar) = (sheet_drag_handle.clone(), top_bar.clone());
            let (snackbar_host, content) = (snackbar_host.clone(), content.clone());
            let policy = policy.clone();
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(content_color), move || {
                let (sheet_content, sheet_shape) = (sheet_content.clone(), sheet_shape.clone());
                let (sheet_drag_handle, top_bar) = (sheet_drag_handle.clone(), top_bar.clone());
                let (snackbar_host, content) = (snackbar_host.clone(), content.clone());
                Layout(
                    Modifier,
                    policy.clone(),
                    compose_runtime::callback(move || {
                        let top_bar = top_bar.clone();
                        slot(move || {
                            if let Some(top_bar) = top_bar.clone() {
                                top_bar();
                            }
                        });
                        let content = content.clone();
                        slot(move || {
                            content(PaddingValues::new(
                                Dp(0.0),
                                Dp(0.0),
                                Dp(0.0),
                                sheet_peek_height,
                            ))
                        });
                        standard_bottom_sheet(
                            state,
                            sheet_peek_height,
                            sheet_max_width,
                            sheet_swipe_enabled,
                            sheet_shape.clone(),
                            sheet_container_color,
                            sheet_content_color,
                            sheet_tonal_elevation,
                            sheet_shadow_elevation,
                            sheet_drag_handle.clone(),
                            sheet_content.clone(),
                        );
                        let snackbar_host = snackbar_host.clone();
                        slot(move || snackbar_host(host_state));
                    }),
                );
            });
        }),
    );
}

/// One of the layout's children: what it emits is one node.
fn slot(content: impl Fn() + 'static) {
    Box(Modifier, Alignment::TOP_START, false, compose_runtime::callback(move |_| content()));
}

/// As Compose's `StandardBottomSheet`.
#[allow(clippy::too_many_arguments)]
fn standard_bottom_sheet(
    state: SheetState,
    peek_height: Dp,
    max_width: Dp,
    swipe_enabled: bool,
    shape: Shape,
    container_color: Color,
    content_color: Color,
    tonal_elevation: Dp,
    shadow_elevation: Dp,
    drag_handle: Option<compose_runtime::Callback>,
    content: compose_runtime::Callback<dyn Fn(ColumnScope)>,
) {
    let scope = remember_coroutine_scope();
    let motion = MaterialTheme::motion_scheme();
    compose_runtime::SideEffect(move || state.set_motion(&motion));
    let fling = state.remember_settling();
    let connection = {
        let fling = fling.clone();
        compose_runtime::remember_keyed((state, FlingKey(fling.clone())), move || {
            Rc::new(SheetConnection { state, fling }) as Rc<dyn NestedScrollConnection>
        })
    };
    let modifier = Modifier
        .size_in(SizeSpec::bounds(Dp::UNSPECIFIED, Dp::UNSPECIFIED, max_width, Dp::UNSPECIFIED))
        .fill_max_width()
        .size_in(SizeSpec {
            enforce_incoming: false,
            ..SizeSpec::bounds(Dp::UNSPECIFIED, peek_height, Dp::UNSPECIFIED, Dp::UNSPECIFIED)
        });
    let modifier = if swipe_enabled { modifier.nested_scroll(connection) } else { modifier };
    let modifier = modifier.anchored_draggable(
        state.anchored_state(),
        Orientation::Vertical,
        AnchoredDraggableOptions {
            enabled: swipe_enabled,
            fling_behavior: Some(fling),
            ..AnchoredDraggableOptions::default()
        },
    );
    crate::components::Surface(
        modifier,
        shape,
        container_color,
        content_color,
        tonal_elevation,
        shadow_elevation,
        None,
        compose_runtime::callback(move || {
            let (drag_handle, content) = (drag_handle.clone(), content.clone());
            Column(
                Modifier.fill_max_width(),
                compose_foundation::layout::Arrangement::TOP,
                Alignment::START,
                compose_runtime::callback(move |column: ColumnScope| {
                    if let Some(drag_handle) = drag_handle.clone() {
                        Box(
                            Modifier
                                .align(column, Alignment::CENTER_HORIZONTALLY)
                                .clickable(move || {
                                    let skips_hidden =
                                        state.inner().is_none_or(|inner| inner.skip_hidden_state);
                                    scope.launch(async move {
                                        match state.current_value() {
                                            SheetValue::Expanded if !skips_hidden => {
                                                state.hide().await
                                            }
                                            SheetValue::Expanded => state.partial_expand().await,
                                            SheetValue::PartiallyExpanded => state.expand().await,
                                            SheetValue::Hidden => state.show().await,
                                        }
                                    });
                                })
                                .then_custom(crate::sheet::HandleSemantics(Rc::new(
                                    move |config: &mut compose_ui::SemanticsConfiguration| {
                                        handle_semantics(state, swipe_enabled, scope, config)
                                    },
                                ))),
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
}

/// The drag handle's actions, while there's more than one anchor to go to and swiping is on, as
/// `StandardBottomSheet`'s semantics. Each answers whether `confirm_value_change` let it.
fn handle_semantics(
    state: SheetState,
    swipe_enabled: bool,
    scope: compose_runtime::CoroutineScope,
    config: &mut compose_ui::SemanticsConfiguration,
) {
    let Some(inner) = state.inner() else { return };
    if inner.anchored.anchors().size() <= 1 || !swipe_enabled {
        return;
    }
    if state.current_value() == SheetValue::PartiallyExpanded {
        config.expand(Some("Expand bottom sheet".into()), move || {
            let can_expand = state.confirms(SheetValue::Expanded);
            if can_expand {
                scope.launch(async move { state.expand().await });
            }
            can_expand
        });
    } else {
        config.collapse(Some("Collapse bottom sheet".into()), move || {
            let can_partially_expand = state.confirms(SheetValue::PartiallyExpanded);
            scope.launch(async move { state.partial_expand().await });
            can_partially_expand
        });
    }
    if !inner.skip_hidden_state {
        config.dismiss(Some("Dismiss bottom sheet".into()), move || {
            let can_hide = state.confirms(SheetValue::Hidden);
            scope.launch(async move { state.hide().await });
            can_hide
        });
    }
}

/// As `BottomSheetScaffoldLayout`, with the sheet's `draggableAnchors`: the top bar, the body
/// below it, the sheet at the state's offset, the snackbars above the sheet while it peeks.
struct ScaffoldLayout {
    state: SheetState,
    peek_height: Dp,
}

impl MeasurePolicy for ScaffoldLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let [top_bar, body, sheet, snackbar] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let (width, height) = (constraints.max_width, constraints.max_height);
        let loose = constraints.loosen();
        let sheet = scope.measure(*sheet, loose);
        let top_bar = scope.measure(*top_bar, loose);
        let body =
            scope.measure(*body, Constraints::new(0, width, 0, (height - top_bar.height).max(0)));
        let snackbar = scope.measure(*snackbar, loose);
        let peek = scope.density().to_px(self.peek_height);
        // What the anchors depend on (`update_anchors` reads it unobserved), and measures again.
        let _ = self.state.current_value();
        self.update_anchors(height as f32, sheet.height as f32, peek);
        let state = self.state;
        scope.layout_with(width, height, move |placement| {
            // Read while placing: a frame of the slide places the sheet again only.
            let offset = state.inner().map_or(f32::NAN, |inner| inner.anchored.offset());
            let top = if offset.is_nan() { height as f32 } else { offset };
            let snackbar_y = match state.current_value() {
                SheetValue::PartiallyExpanded => top.round() as i32 - snackbar.height,
                SheetValue::Expanded | SheetValue::Hidden => height - snackbar.height,
            };
            // In this order: for elevation.
            placement.place_relative(&body, 0, top_bar.height);
            placement.place_relative(&top_bar, 0, 0);
            placement.place_relative(
                &sheet,
                ((width - sheet.width) / 2).max(0),
                top.round() as i32,
            );
            placement.place_relative(&snackbar, (width - snackbar.width) / 2, snackbar_y);
        })
    }

    fn debug_name(&self) -> &'static str {
        "BottomSheetScaffold"
    }
}

impl ScaffoldLayout {
    /// The sheet's anchors for its size, as `StandardBottomSheet`'s `draggableAnchors`.
    fn update_anchors(&self, layout_height: f32, sheet_height: f32, peek: f32) {
        let Some(inner) = self.state.inner() else { return };
        let anchored = &inner.anchored;
        let hidden_available = sheet_height == 0.0 || peek == 0.0 || !inner.skip_hidden_state;
        let (anchors, new_target) =
            compose_runtime::snapshots::Snapshot::without_read_observation(|| {
                // A peek height of 0 can be a value not yet computed: on the first layout, or resting
                // there, it stays at the anchor apps know of, as Compose's.
                let is_initial_layout = anchored.anchors().size() == 0;
                let is_stable_at_partial = self.state.current_value()
                    == SheetValue::PartiallyExpanded
                    && !anchored.is_animation_running();
                let ambiguous_partial_allowed =
                    peek == 0.0 && (is_initial_layout || is_stable_at_partial);
                let partial_available = !inner.skip_partially_expanded
                    && (peek > 0.0 || ambiguous_partial_allowed)
                    && peek != sheet_height;
                let expanded_available = sheet_height > 0.0;
                assert!(
                    hidden_available || partial_available || expanded_available,
                    "BottomSheetScaffold: Require at least 1 anchor to be initialized"
                );
                let anchors = DraggableAnchors::new(|config| {
                    if partial_available {
                        config.at(SheetValue::PartiallyExpanded, layout_height - peek);
                    }
                    if hidden_available {
                        config.at(SheetValue::Hidden, layout_height);
                    }
                    if expanded_available {
                        config.at(SheetValue::Expanded, layout_height - sheet_height);
                    }
                });
                let has = |value: SheetValue| anchors.has_position_for(&value);
                let old_target = self.state.target_value();
                let new_target = match old_target {
                    SheetValue::Hidden => {
                        if has(SheetValue::Hidden) {
                            SheetValue::Hidden
                        } else {
                            old_target
                        }
                    }
                    SheetValue::PartiallyExpanded => {
                        if has(SheetValue::PartiallyExpanded) {
                            SheetValue::PartiallyExpanded
                        } else if has(SheetValue::Expanded) {
                            SheetValue::Expanded
                        } else if has(SheetValue::Hidden) {
                            SheetValue::Hidden
                        } else {
                            old_target
                        }
                    }
                    SheetValue::Expanded => {
                        if has(SheetValue::Expanded) {
                            SheetValue::Expanded
                        } else {
                            SheetValue::Hidden
                        }
                    }
                };
                (anchors, new_target)
            });
        self.state.update_anchors(anchors, new_target);
    }
}
