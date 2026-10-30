//! `SwipeToDismissBox`, as Compose's Material 3: content swiped away sideways, uncovering a
//! background, with its `SwipeToDismissBoxState`.

use compose_runtime::{ObjectHandle, Owned};
use std::rc::Rc;

use compose_foundation::layout::{Arrangement, Row, RowScope};

use compose_foundation::gestures::{
    AnchoredDraggableDefaults, AnchoredDraggableModifier, AnchoredDraggableOptions,
    AnchoredDraggableState, DraggableAnchors, Orientation, remember_anchored_fling_behavior,
};
use compose_runtime::{ComposeArg, composable};
use compose_ui::{
    Alignment, Constraints, Dp, Layout, Measurable, MeasurePolicy, MeasureResult, MeasureScope,
    Modifier,
};

/// Where a swipe-to-dismiss box rests, as Compose's `SwipeToDismissBoxValue`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwipeToDismissBoxValue {
    /// Swiped away towards the end.
    StartToEnd,
    /// Swiped away towards the start.
    EndToStart,
    /// In place.
    Settled,
}

impl SwipeToDismissBoxValue {
    fn from_key(key: u8) -> SwipeToDismissBoxValue {
        match key {
            0 => SwipeToDismissBoxValue::StartToEnd,
            1 => SwipeToDismissBoxValue::EndToStart,
            _ => SwipeToDismissBoxValue::Settled,
        }
    }
}

impl ComposeArg for SwipeToDismissBoxValue {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

struct SwipeInner {
    anchored: AnchoredDraggableState<SwipeToDismissBoxValue>,
    positional_threshold: Rc<dyn Fn(f32) -> f32>,
}

/// A swipe-to-dismiss box's state, as Compose's `SwipeToDismissBoxState`.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_swipe_to_dismiss_box_state`] keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SwipeToDismissBoxState {
    object: ObjectHandle<SwipeInner>,
}

impl SwipeToDismissBoxState {
    /// Its object, or `None` once its owner went (D240): then it does nothing, its reads
    /// giving the defaults.
    #[track_caller]
    fn inner(&self) -> Option<Rc<SwipeInner>> {
        self.object.get()
    }

    /// As `SwipeToDismissBoxState(initialValue, positionalThreshold)`: `positional_threshold`
    /// gives, from the distance to the next anchor (the width), how far (pixels) a swipe must
    /// go. Compose's overload with `confirmValueChange` is deprecated (`SwipeToDismissBox`'s
    /// `on_dismiss` and `gestures_enabled` replace it), and not here (D1001).
    #[track_caller]
    pub fn new(
        initial_value: SwipeToDismissBoxValue,
        positional_threshold: impl Fn(f32) -> f32 + 'static,
    ) -> Owned<SwipeToDismissBoxState> {
        Owned::new(
            SwipeInner {
                anchored: AnchoredDraggableState::new(initial_value),
                positional_threshold: Rc::new(positional_threshold),
            },
            |object| SwipeToDismissBoxState { object },
        )
    }

    /// Where it last settled, or, while swiped, the anchor it was swiped half way to (a state
    /// read), as `currentValue`.
    pub fn current_value(&self) -> SwipeToDismissBoxValue {
        self.inner().map_or(SwipeToDismissBoxValue::Settled, |inner| inner.anchored.current_value())
    }

    /// Where it last settled (a state read), as `settledValue`.
    pub fn settled_value(&self) -> SwipeToDismissBoxValue {
        self.inner().map_or(SwipeToDismissBoxValue::Settled, |inner| inner.anchored.settled_value())
    }

    /// Where it's going, or would settle if let go now (a state read), as `targetValue`.
    pub fn target_value(&self) -> SwipeToDismissBoxValue {
        self.inner().map_or(SwipeToDismissBoxValue::Settled, |inner| inner.anchored.target_value())
    }

    /// The content's offset, in pixels (a state read), as `requireOffset()`.
    pub fn require_offset(&self) -> f32 {
        self.inner().map_or(0.0, |inner| inner.anchored.offset())
    }

    /// Which way it's being swiped, from the (logical) offset's sign, as `dismissDirection`.
    pub fn dismiss_direction(&self) -> SwipeToDismissBoxValue {
        let offset = self.require_offset();
        if offset == 0.0 || offset.is_nan() {
            SwipeToDismissBoxValue::Settled
        } else if offset > 0.0 {
            SwipeToDismissBoxValue::StartToEnd
        } else {
            SwipeToDismissBoxValue::EndToStart
        }
    }

    /// How far it's been swiped towards a dismissal, 0 to 1, as `progress`.
    pub fn progress(&self) -> f32 {
        let Some(inner) = self.inner() else { return 0.0 };
        let anchored = &inner.anchored;
        anchored.progress(&anchored.settled_value(), &anchored.target_value())
    }

    /// Animates it back in place, as `reset()`.
    pub async fn reset(&self) {
        if let Some(inner) = self.inner() {
            let _ = inner.anchored.animate_to(SwipeToDismissBoxValue::Settled).await;
        }
    }

    /// Animates it away in `direction`, as `dismiss(direction)`.
    pub async fn dismiss(&self, direction: SwipeToDismissBoxValue) {
        if let Some(inner) = self.inner() {
            let _ = inner.anchored.animate_to(direction).await;
        }
    }

    /// At `value` at once, as `snapTo`.
    pub async fn snap_to(&self, value: SwipeToDismissBoxValue) {
        if let Some(inner) = self.inner() {
            let _ = inner.anchored.snap_to(value).await;
        }
    }
}

impl ComposeArg for SwipeToDismissBoxState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl SwipeToDismissBoxState {
    /// Saves where it last settled, as Compose's `SwipeToDismissBoxState.Saver(
    /// positionalThreshold)`.
    pub fn saver(
        positional_threshold: impl Fn(f32) -> f32 + 'static,
    ) -> impl compose_runtime::Saver<Owned<SwipeToDismissBoxState>> {
        let positional_threshold = Rc::new(positional_threshold);
        compose_runtime::saver(
            |state: &Owned<SwipeToDismissBoxState>| {
                state.inner()?;
                Some(compose_runtime::SavedValue::Int(state.current_value() as i64))
            },
            move |saved| match saved {
                compose_runtime::SavedValue::Int(key) if (0..=2).contains(key) => {
                    let positional_threshold = positional_threshold.clone();
                    Some(SwipeToDismissBoxState::new(
                        SwipeToDismissBoxValue::from_key(*key as u8),
                        move |distance| positional_threshold(distance),
                    ))
                }
                _ => None,
            },
        )
    }
}

/// As Compose's `SwipeToDismissBoxDefaults`.
pub struct SwipeToDismissBoxDefaults;

impl SwipeToDismissBoxDefaults {
    /// How far a swipe must go to dismiss: 56 dp whatever the distance, as
    /// `positionalThreshold` (read in composition: the density's).
    pub fn positional_threshold() -> compose_runtime::Callback<dyn Fn(f32) -> f32> {
        let threshold = compose_ui::LOCAL_DENSITY.current().to_px(Dp(56.0));
        compose_runtime::callback(move |_: f32| threshold)
    }
}

/// A swipe-to-dismiss state remembered here, as `rememberSwipeToDismissBoxState(initialValue,
/// positionalThreshold)`: saved across the host's recreation (a `rememberSaveable`, as
/// Compose's). `positional_threshold` is the first composition's.
#[composable(non_restartable)]
pub fn remember_swipe_to_dismiss_box_state(
    #[default(crate::SwipeToDismissBoxValue::Settled)] initial_value: SwipeToDismissBoxValue,
    #[default(crate::SwipeToDismissBoxDefaults::positional_threshold())]
    positional_threshold: impl Fn(f32) -> f32 + 'static,
) -> SwipeToDismissBoxState {
    let restored = positional_threshold.clone();
    #[allow(clippy::redundant_closure)] // A `Callback` derefs to the function; it isn't one.
    compose_runtime::remember_saveable_with(
        SwipeToDismissBoxState::saver(move |distance| restored(distance)),
        move || {
            SwipeToDismissBoxState::new(initial_value, move |distance| {
                positional_threshold(distance)
            })
        },
    )
}

/// Content that can be swiped away sideways, as Compose's `SwipeToDismissBox`: dragged across,
/// it uncovers `background_content` (as large as the content, behind it), and let go past the
/// threshold or fast enough it settles away (`StartToEnd` or `EndToStart`, each if enabled), else
/// back in place. Once it settles away, `on_dismiss` hears the direction: remove the item then.
/// A dismissed item can't be swiped (reset its state to bring it back).
#[composable]
pub fn SwipeToDismissBox(
    state: SwipeToDismissBoxState,
    background_content: impl Fn(RowScope) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enable_dismiss_from_start_to_end: bool,
    #[default(true)] enable_dismiss_from_end_to_start: bool,
    #[default(true)] gestures_enabled: bool,
    #[default(|_| {})] on_dismiss: impl Fn(SwipeToDismissBoxValue) + 'static,
    content: impl Fn(RowScope) + 'static,
) {
    // Tells `on_dismiss` once settled away, as Compose's `LaunchedEffect(settledValue,
    // onDismiss)`.
    let settled = state.settled_value();
    {
        let on_dismiss = on_dismiss.clone();
        compose_runtime::LaunchedEffect((settled, on_dismiss.clone()), async move {
            if settled != SwipeToDismissBoxValue::Settled {
                on_dismiss(state.dismiss_direction());
            }
        });
    }
    let (anchored, threshold) = match state.inner() {
        Some(inner) => (inner.anchored.clone(), inner.positional_threshold.clone()),
        None => (
            AnchoredDraggableState::new(SwipeToDismissBoxValue::Settled),
            Rc::new(|_: f32| 0.0f32) as Rc<dyn Fn(f32) -> f32>,
        ),
    };
    let fling = remember_anchored_fling_behavior(
        anchored.clone(),
        threshold,
        AnchoredDraggableDefaults::snap_animation_spec(),
    );
    let policy = {
        compose_runtime::remember_keyed(
            (state, enable_dismiss_from_start_to_end, enable_dismiss_from_end_to_start),
            move || {
                Rc::new(SwipeLayout {
                    state,
                    start_to_end: enable_dismiss_from_start_to_end,
                    end_to_start: enable_dismiss_from_end_to_start,
                }) as Rc<dyn MeasurePolicy>
            },
        )
    };
    Layout(
        // Dragged only while settled in place, as Compose's (a dismissed item stays away); the
        // offset is logical (towards the end positive), so a right-to-left drag is reversed, as the
        // modifier's default.
        modifier.anchored_draggable(
            anchored,
            Orientation::Horizontal,
            AnchoredDraggableOptions {
                enabled: gestures_enabled && settled == SwipeToDismissBoxValue::Settled,
                fling_behavior: Some(fling),
                ..AnchoredDraggableOptions::default()
            },
        ),
        policy,
        compose_runtime::callback(move || {
            Row(
                Modifier,
                Arrangement::START,
                Alignment::CENTER_VERTICALLY,
                background_content.clone(),
            );
            Row(Modifier, Arrangement::START, Alignment::CENTER_VERTICALLY, content.clone());
        }),
    );
}

/// The background behind, as large as the content; the content at the state's (logical)
/// offset. The anchors: in place, and a width away each way enabled (towards the end positive,
/// placed mirrored in a right-to-left layout).
struct SwipeLayout {
    state: SwipeToDismissBoxState,
    start_to_end: bool,
    end_to_start: bool,
}

impl MeasurePolicy for SwipeLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let [background, content] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let content = scope.measure(*content, constraints);
        let background =
            scope.measure(*background, Constraints::fixed(content.width, content.height));
        let width = content.width as f32;
        let towards_end = width;
        let (start_to_end, end_to_start) = (self.start_to_end, self.end_to_start);
        let anchors = DraggableAnchors::new(|config| {
            config.at(SwipeToDismissBoxValue::Settled, 0.0);
            if start_to_end {
                config.at(SwipeToDismissBoxValue::StartToEnd, towards_end);
            }
            if end_to_start {
                config.at(SwipeToDismissBoxValue::EndToStart, -towards_end);
            }
        });
        let Some(inner) = self.state.inner() else {
            scope.place(&background, 0, 0);
            scope.place_relative(&content, 0, 0);
            return scope.layout(content.width, content.height);
        };
        let anchored = &inner.anchored;
        // Where new anchors take it, as Compose's `draggableAnchors` target.
        compose_runtime::snapshots::Snapshot::without_read_observation(|| {
            let is_initialized = anchored.anchors().size() > 0;
            let (previous, target) = (anchored.current_value(), anchored.target_value());
            let new_target = if !is_initialized && anchors.has_position_for(&previous) {
                previous
            } else if anchors.has_position_for(&target) {
                target
            } else {
                SwipeToDismissBoxValue::Settled
            };
            anchored.update_anchors_to(anchors, new_target);
        });
        let state = self.state;
        scope.layout_with(content.width, content.height, move |placement| {
            // Read while placing: a frame of the swipe places the content again only.
            let offset = state.inner().map_or(0.0, |inner| inner.anchored.offset());
            let offset = if offset.is_nan() { 0 } else { offset.round() as i32 };
            placement.place(&background, 0, 0);
            // Mirrored in a right-to-left layout: towards the end is left there.
            placement.place_relative(&content, offset, 0);
        })
    }

    fn debug_name(&self) -> &'static str {
        "SwipeToDismissBox"
    }
}
