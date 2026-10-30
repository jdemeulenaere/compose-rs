//! `Snackbar`, `SnackbarHost` and `SnackbarHostState`, as Compose's Material 3: brief messages
//! at the bottom of the screen, shown one at a time by `show_snackbar` (a coroutine returning
//! whether the action was performed), each for its duration.

use compose_runtime::{ObjectHandle, Owned};
use compose_ui::SharedString;
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use compose_animation::{
    AnimatedVisibility, fade_in_with, fade_out_with, scale_in_with, scale_out_with,
};
use compose_foundation::layout::{Box, RowScope};
use compose_runtime::{
    ComposeArg, CompositionLocalProvider, LaunchedEffect, OwnedState, composable, remember,
};
use compose_ui::{Alignment, Color, Dp, Modifier, Shape, SizeSpec, UnitExt};

use crate::components::Surface;
use crate::icon::{Icon, IconButton, icons};
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

/// How long a snackbar shows, as Compose's `SnackbarDuration`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SnackbarDuration {
    /// 4 seconds.
    Short,
    /// 10 seconds.
    Long,
    /// Until dismissed or its action is performed.
    Indefinite,
}

impl SnackbarDuration {
    fn millis(self) -> Option<u64> {
        match self {
            SnackbarDuration::Short => Some(4000),
            SnackbarDuration::Long => Some(10000),
            SnackbarDuration::Indefinite => None,
        }
    }
}

/// How a snackbar ended, as Compose's `SnackbarResult`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SnackbarResult {
    /// It timed out or was dismissed.
    Dismissed,
    /// Its action was clicked.
    ActionPerformed,
}

/// What a snackbar shows, as Compose's `SnackbarVisuals`.
#[derive(Clone, PartialEq, Debug)]
pub struct SnackbarVisuals {
    pub message: SharedString,
    pub action_label: Option<SharedString>,
    pub with_dismiss_action: bool,
    pub duration: SnackbarDuration,
}

impl From<SharedString> for SnackbarVisuals {
    /// A message alone, shown for `SnackbarDuration::Short`, as `showSnackbar(message)`.
    fn from(message: SharedString) -> SnackbarVisuals {
        SnackbarVisuals {
            message,
            action_label: None,
            with_dismiss_action: false,
            duration: SnackbarDuration::Short,
        }
    }
}

/// A message alone from what a text parameter takes (D263: a `&'static str`, a `String` or
/// `&String`, an `Rc<str>`, a `SharedString` or `&SharedString`).
macro_rules! message_visuals {
    ($($type:ty),*) => {
        $(impl From<$type> for SnackbarVisuals {
            fn from(message: $type) -> SnackbarVisuals {
                SnackbarVisuals::from(SharedString::from(message))
            }
        })*
    };
}

message_visuals!(&'static str, String, &String, Rc<str>, &SharedString);

/// A snackbar being shown, as Compose's `SnackbarData`: its visuals, and how to end it. A
/// `SnackbarHostState` makes them; [`with_callbacks`](Self::with_callbacks) makes one of your
/// own, as implementing Compose's interface.
#[derive(Clone)]
pub struct SnackbarData {
    pub visuals: SnackbarVisuals,
    done: Rc<Completion>,
    /// Its own `perform_action` and `dismiss`, for one made with `with_callbacks`.
    callbacks: Option<Rc<SnackbarCallbacks>>,
}

/// `SnackbarData::with_callbacks`' callbacks.
struct SnackbarCallbacks {
    perform_action: std::boxed::Box<dyn Fn()>,
    dismiss: std::boxed::Box<dyn Fn()>,
}

impl SnackbarData {
    /// Data for `visuals` whose action and dismissal call `perform_action` and `dismiss`, as
    /// an implementation of Compose's `SnackbarData` interface (a snackbar shown without a host).
    pub fn with_callbacks(
        visuals: SnackbarVisuals,
        perform_action: impl Fn() + 'static,
        dismiss: impl Fn() + 'static,
    ) -> SnackbarData {
        SnackbarData {
            visuals,
            done: Rc::new(Completion::default()),
            callbacks: Some(Rc::new(SnackbarCallbacks {
                perform_action: std::boxed::Box::new(perform_action),
                dismiss: std::boxed::Box::new(dismiss),
            })),
        }
    }

    /// Ends it as performed: `show_snackbar` returns `ActionPerformed`.
    pub fn perform_action(&self) {
        match &self.callbacks {
            Some(callbacks) => (callbacks.perform_action)(),
            None => self.done.complete(SnackbarResult::ActionPerformed),
        }
    }

    /// Ends it as dismissed.
    pub fn dismiss(&self) {
        match &self.callbacks {
            Some(callbacks) => (callbacks.dismiss)(),
            None => self.done.complete(SnackbarResult::Dismissed),
        }
    }
}

impl PartialEq for SnackbarData {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.done, &other.done)
    }
}

impl std::fmt::Debug for SnackbarData {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.visuals.fmt(formatter)
    }
}

impl ComposeArg for SnackbarData {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A result set once, awaited by whoever waits for it.
#[derive(Default)]
struct Completion {
    result: Cell<Option<SnackbarResult>>,
    wakers: RefCell<Vec<Waker>>,
}

impl Completion {
    fn complete(&self, result: SnackbarResult) {
        if self.result.get().is_none() {
            self.result.set(Some(result));
            for waker in self.wakers.take() {
                waker.wake();
            }
        }
    }

    fn wait(self: &Rc<Self>) -> Wait {
        Wait(self.clone())
    }
}

struct Wait(Rc<Completion>);

impl Future for Wait {
    type Output = SnackbarResult;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<SnackbarResult> {
        match self.0.result.get() {
            Some(result) => Poll::Ready(result),
            None => {
                let mut wakers = self.0.wakers.borrow_mut();
                if !wakers.iter().any(|waker| waker.will_wake(context.waker())) {
                    wakers.push(context.waker().clone());
                }
                Poll::Pending
            }
        }
    }
}

/// The snackbars to show, one at a time, as Compose's `SnackbarHostState`; remember one
/// (`remember(SnackbarHostState::new)`) and give it to a `SnackbarHost`.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, which
/// `remember(SnackbarHostState::new)` keeps in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SnackbarHostState {
    object: ObjectHandle<HostInner>,
}

struct HostInner {
    current: OwnedState<Option<SnackbarData>>,
    /// The snackbars asked for and not ended, in order, as Compose's `Mutex` queue: the first
    /// shows; a cancelled one leaves its place.
    queue: RefCell<std::collections::VecDeque<Rc<Completion>>>,
    /// Who waits for the queue to move.
    waiting: RefCell<Vec<Waker>>,
}

impl HostInner {
    /// Wakes the waiting ones: the queue moved.
    fn moved(&self) {
        for waker in self.waiting.take() {
            waker.wake();
        }
    }
}

/// Waits for `done` to be first in the queue.
struct Turn {
    host: Rc<HostInner>,
    done: Rc<Completion>,
}

impl Future for Turn {
    type Output = ();
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        let first =
            self.host.queue.borrow().front().is_some_and(|first| Rc::ptr_eq(first, &self.done));
        if first {
            Poll::Ready(())
        } else {
            self.host.waiting.borrow_mut().push(context.waker().clone());
            Poll::Pending
        }
    }
}

impl ComposeArg for SnackbarHostState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl SnackbarHostState {
    /// Its object, or `None` once its owner went (D240): then it does nothing, its reads
    /// giving the defaults.
    #[track_caller]
    fn inner(&self) -> Option<Rc<HostInner>> {
        self.object.get()
    }

    #[track_caller]
    pub fn new() -> Owned<SnackbarHostState> {
        Owned::new(
            HostInner {
                current: compose_runtime::mutable_state_of(None),
                queue: RefCell::new(std::collections::VecDeque::new()),
                waiting: RefCell::new(Vec::new()),
            },
            |object| SnackbarHostState { object },
        )
    }

    /// The snackbar showing, if any, as `currentSnackbarData` (a state read).
    pub fn current_snackbar_data(&self) -> Option<SnackbarData> {
        self.inner().and_then(|inner| inner.current.get())
    }

    /// Shows a snackbar once those asked for before it ended, and waits for it to end, as
    /// `showSnackbar`: `ActionPerformed` if its action was clicked. Cancelling the coroutine
    /// takes it away.
    pub async fn show_snackbar(&self, visuals: impl Into<SnackbarVisuals>) -> SnackbarResult {
        self.show(visuals.into()).await
    }

    /// `show_snackbar` with Compose's optional parameters, as `showSnackbar(message,
    /// actionLabel, withDismissAction, duration)`.
    pub async fn show_snackbar_with(
        &self,
        message: impl Into<SharedString>,
        action_label: Option<SharedString>,
        with_dismiss_action: bool,
        duration: SnackbarDuration,
    ) -> SnackbarResult {
        self.show(SnackbarVisuals {
            message: message.into(),
            action_label,
            with_dismiss_action,
            duration,
        })
        .await
    }

    async fn show(&self, visuals: SnackbarVisuals) -> SnackbarResult {
        let done = Rc::new(Completion::default());
        // Kept while it waits: the host outlives its owner until the snackbar ends.
        let Some(host) = self.inner() else { return SnackbarResult::Dismissed };
        host.queue.borrow_mut().push_back(done.clone());
        // Ends it however this coroutine ends: it leaves the queue, the next ones go on.
        let guard = Guard { host: host.clone(), done: done.clone() };
        Turn { host: host.clone(), done: done.clone() }.await;
        let data = SnackbarData { visuals, done: done.clone(), callbacks: None };
        host.current.set(Some(data));
        let result = done.wait().await;
        drop(guard);
        result
    }
}

/// Takes a snackbar away when its `show_snackbar` ends or is cancelled.
struct Guard {
    host: Rc<HostInner>,
    done: Rc<Completion>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.done.complete(SnackbarResult::Dismissed);
        let ours = self.host.current.get().is_some_and(|data| Rc::ptr_eq(&data.done, &self.done));
        if ours {
            self.host.current.set(None);
        }
        self.host.queue.borrow_mut().retain(|queued| !Rc::ptr_eq(queued, &self.done));
        self.host.moved();
    }
}

/// Shows `host_state`'s snackbars, as Compose's `SnackbarHost`: each faded and scaled in, then
/// out, dismissed once its duration is over. `snackbar` draws one (by default a `Snackbar`).
#[composable]
#[allow(non_snake_case)]
pub fn SnackbarHost(
    host_state: SnackbarHostState,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] snackbar: Option<impl Fn(SnackbarData) + 'static>,
) {
    let current = host_state.current_snackbar_data();
    // Its duration: then it's dismissed.
    LaunchedEffect(current.clone(), {
        let current = current.clone();
        async move {
            if let Some(data) = current
                && let Some(millis) = data.visuals.duration.millis()
            {
                compose_runtime::delay(std::time::Duration::from_millis(millis)).await;
                data.dismiss();
            }
        }
    });
    // The last one shown, drawn while it fades out.
    // Kept as Compose's `FadeInFadeOutWithScale` keeps its items; captured by the content, so
    // a new one composes it again.
    let last = remember(|| Rc::new(RefCell::new(None::<SnackbarData>)));
    if current.is_some() {
        *last.borrow_mut() = current.clone();
    }
    let shown = last.borrow().clone();
    let motion = crate::theme::MaterialTheme::motion_scheme();
    AnimatedVisibility(
        current.is_some(),
        modifier,
        // As `FadeInFadeOutWithScale`: its opacity on the fast effects spec, its scale between
        // 0.8 and 1 on the fast spatial one.
        Some(
            fade_in_with(motion.fast_effects_spec(), 0.0)
                + scale_in_with(
                    motion.fast_spatial_spec(),
                    0.8,
                    compose_ui::TransformOrigin::CENTER,
                ),
        ),
        Some(
            fade_out_with(motion.fast_effects_spec(), 0.0)
                + scale_out_with(
                    motion.fast_spatial_spec(),
                    0.8,
                    compose_ui::TransformOrigin::CENTER,
                ),
        ),
        "SnackbarHost",
        compose_runtime::callback(move |_| {
            let Some(data) = shown.clone() else { return };
            // As `FadeInFadeOutWithScale`'s item: announced while it's the one shown, and
            // dismissed by accessibility.
            let visible = host_state.current_snackbar_data().is_some();
            let on_dismiss = data.clone();
            let snackbar = snackbar.clone();
            compose_foundation::layout::Box(
                Modifier.semantics(move |config| {
                    if visible {
                        config.set_live_region(compose_ui::LiveRegionMode::Polite);
                    }
                    let on_dismiss = on_dismiss.clone();
                    config.dismiss(None, move || {
                        on_dismiss.dismiss();
                        true
                    });
                    config.set_pane_title("Alert");
                }),
                compose_ui::Alignment::TOP_START,
                false,
                compose_runtime::callback(move |_| {
                    let data = data.clone();
                    match snackbar.clone() {
                        Some(snackbar) => snackbar(data),
                        None => Snackbar(
                            data,
                            Modifier,
                            false,
                            SnackbarDefaults::shape(),
                            SnackbarDefaults::color(),
                            SnackbarDefaults::content_color(),
                            SnackbarDefaults::action_color(),
                            SnackbarDefaults::action_content_color(),
                            SnackbarDefaults::dismiss_action_content_color(),
                        ),
                    }
                }),
            );
        }),
    );
}

/// As Compose's `SnackbarDefaults` (`SnackbarTokens`).
pub struct SnackbarDefaults;

impl SnackbarDefaults {
    /// The extra-small shape, as `shape`.
    pub fn shape() -> Shape {
        MaterialTheme::shapes().extra_small.clone()
    }

    /// The inverse surface, as `color`.
    pub fn color() -> Color {
        MaterialTheme::color_scheme().inverse_surface
    }

    /// The inverse surface's content, as `contentColor`.
    pub fn content_color() -> Color {
        MaterialTheme::color_scheme().inverse_on_surface
    }

    /// The inverse primary color, as `actionColor`: the data overload's action button's.
    pub fn action_color() -> Color {
        MaterialTheme::color_scheme().inverse_primary
    }

    /// The inverse primary color, as `actionContentColor`: around the action.
    pub fn action_content_color() -> Color {
        MaterialTheme::color_scheme().inverse_primary
    }

    /// The inverse surface's content, as `dismissActionContentColor`.
    pub fn dismiss_action_content_color() -> Color {
        MaterialTheme::color_scheme().inverse_on_surface
    }
}

/// A snackbar of any content, as Compose's `Snackbar(action, dismissAction, actionOnNewLine, …,
/// content)` (named apart from the data overload, D181): `content` (in the body medium style)
/// and the optional `action` (in `action_content_color` and the label large style) and
/// `dismiss_action` (in `dismiss_action_content_color`) on the inverse surface, at most 600 dp
/// wide, at least 48 dp high (68 dp for two lines), with a 6 dp shadow; the action under the
/// content, at the end, with `action_on_new_line`.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn ContentSnackbar(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] action: Option<impl Fn() + 'static>,
    #[default(None)] dismiss_action: Option<impl Fn() + 'static>,
    #[default(false)] action_on_new_line: bool,
    #[default(crate::SnackbarDefaults::shape())] shape: Shape,
    #[default(crate::SnackbarDefaults::color())] container_color: Color,
    #[default(crate::SnackbarDefaults::content_color())] content_color: Color,
    #[default(crate::SnackbarDefaults::action_content_color())] action_content_color: Color,
    #[default(crate::SnackbarDefaults::dismiss_action_content_color())]
    dismiss_action_content_color: Color,
    content: impl Fn() + 'static,
) {
    let typography = MaterialTheme::typography();
    let (has_action, has_dismiss) = (action.is_some(), dismiss_action.is_some());
    let new_line = action_on_new_line && has_action;
    let policy = compose_runtime::remember_keyed((has_action, has_dismiss, new_line), || {
        if new_line {
            Rc::new(NewLineSnackbarLayout { dismiss: has_dismiss })
                as Rc<dyn compose_ui::MeasurePolicy>
        } else {
            Rc::new(SnackbarLayout {
                action: has_action,
                dismiss: has_dismiss,
                line_height: typography.body_medium.line_height,
            }) as Rc<dyn compose_ui::MeasurePolicy>
        }
    });
    Surface(
        modifier,
        shape,
        container_color,
        content_color,
        Dp(0.0),
        6.dp(),
        None,
        compose_runtime::callback(move || {
            let (action, dismiss, content, policy) =
                (action.clone(), dismiss_action.clone(), content.clone(), policy.clone());
            CompositionLocalProvider(
                LOCAL_TEXT_STYLE.provides(typography.body_medium.clone()),
                || {
                    let (action, dismiss, content) =
                        (action.clone(), dismiss.clone(), content.clone());
                    compose_ui::Layout(
                        Modifier,
                        policy.clone(),
                        compose_runtime::callback(move || {
                            let content = content.clone();
                            Box(
                                Modifier,
                                Alignment::CENTER_START,
                                false,
                                compose_runtime::callback(move |_| content()),
                            );
                            if let Some(action) = action.clone() {
                                CompositionLocalProvider(
                                    (
                                        LOCAL_CONTENT_COLOR.provides(action_content_color),
                                        LOCAL_TEXT_STYLE.provides(typography.label_large.clone()),
                                    ),
                                    || {
                                        let action = action.clone();
                                        Box(
                                            Modifier,
                                            Alignment::TOP_START,
                                            false,
                                            compose_runtime::callback(move |_| action()),
                                        );
                                    },
                                );
                            }
                            if let Some(dismiss) = dismiss.clone() {
                                CompositionLocalProvider(
                                    LOCAL_CONTENT_COLOR.provides(dismiss_action_content_color),
                                    || {
                                        let dismiss = dismiss.clone();
                                        Box(
                                            Modifier,
                                            Alignment::TOP_START,
                                            false,
                                            compose_runtime::callback(move |_| dismiss()),
                                        );
                                    },
                                );
                            }
                        }),
                    );
                },
            );
        }),
    );
}

/// A snackbar of `snackbar_data`, as Compose's `Snackbar(snackbarData, …)`: its message, its
/// action label as a text button in `action_color`, a dismiss button if it asks for one, 12 dp
/// in from what's around it, as `ContentSnackbar`.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn Snackbar(
    snackbar_data: SnackbarData,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(false)] action_on_new_line: bool,
    #[default(crate::SnackbarDefaults::shape())] shape: Shape,
    #[default(crate::SnackbarDefaults::color())] container_color: Color,
    #[default(crate::SnackbarDefaults::content_color())] content_color: Color,
    #[default(crate::SnackbarDefaults::action_color())] action_color: Color,
    #[default(crate::SnackbarDefaults::action_content_color())] action_content_color: Color,
    #[default(crate::SnackbarDefaults::dismiss_action_content_color())]
    dismiss_action_content_color: Color,
) {
    let visuals = snackbar_data.visuals.clone();
    let action = visuals.action_label.clone().map(|label| {
        let data = snackbar_data.clone();
        compose_runtime::callback(move || {
            // The action in its color, as `textButtonColors(contentColor = actionColor)`.
            let (data, label) = (data.clone(), label.clone());
            crate::button::Button(
                compose_runtime::callback(move || data.perform_action()),
                Modifier,
                true,
                crate::ButtonDefaults::shape(),
                None,
                crate::ButtonColors {
                    content_color: action_color,
                    ..crate::ButtonDefaults::text_button_colors()
                },
                None,
                None,
                Some(crate::ButtonDefaults::TEXT_BUTTON_CONTENT_PADDING),
                None,
                compose_runtime::callback(move |_: RowScope| themed_text(label.clone())),
            );
        })
    });
    let dismiss = visuals.with_dismiss_action.then(|| {
        let data = snackbar_data.clone();
        compose_runtime::callback(move || {
            let data = data.clone();
            IconButton(
                compose_runtime::callback(move || data.dismiss()),
                Modifier,
                true,
                crate::IconButtonDefaults::icon_button_colors(),
                None,
                crate::IconButtonDefaults::standard_shape(),
                None,
                compose_runtime::callback(|| {
                    Icon(
                        icons::filled::close(),
                        Some("Dismiss".into()),
                        Modifier,
                        LOCAL_CONTENT_COLOR.current(),
                    )
                }),
            );
        })
    });
    let message = visuals.message.clone();
    // 12 dp around it, as Compose's `Snackbar(snackbarData)`.
    ContentSnackbar(
        modifier.padding(12.dp()),
        action,
        dismiss,
        action_on_new_line,
        shape,
        container_color,
        content_color,
        action_content_color,
        dismiss_action_content_color,
        compose_runtime::callback(move || themed_text(message.clone())),
    );
}

/// A snackbar's row, as Compose's `OneRowSnackbar`: the message 16 dp from the start, 6 dp
/// above and below, taking what the buttons leave; the action, then the dismiss button, at
/// the end (8 dp in after the action alone, 0 dp after the dismiss button, 16 dp after the
/// message alone); at most 600 dp wide; at least 48 dp high, 68 dp when the message takes two
/// lines or more, all centered down it.
struct SnackbarLayout {
    action: bool,
    dismiss: bool,
    line_height: compose_ui::TextUnit,
}

impl compose_ui::MeasurePolicy for SnackbarLayout {
    fn measure(
        &self,
        scope: &mut compose_ui::MeasureScope<'_>,
        measurables: &[compose_ui::Measurable],
        constraints: compose_ui::Constraints,
    ) -> compose_ui::MeasureResult {
        let density = scope.density();
        let px = |dp: f32| density.round_to_px(Dp(dp));
        let start = px(16.0);
        let end = match (self.action, self.dismiss) {
            (_, true) => 0,
            (true, false) => px(8.0),
            (false, false) => px(16.0),
        };
        let vertical = px(6.0);
        let loose = constraints.loosen();
        let buttons: Vec<_> =
            measurables[1..].iter().map(|&button| scope.measure(button, loose)).collect();
        let buttons_width: i32 = buttons.iter().map(|button| button.width).sum();
        // At most 600 dp, as Compose's `OneRowSnackbar` (`ContainerMaxWidth`, D1207).
        let width = constraints.max_width.min(px(600.0)).max(constraints.min_width);
        let text_width = (width - start - end - buttons_width).max(0);
        let text = scope.measure(
            measurables[0],
            compose_ui::Constraints::new(0, text_width, 0, compose_ui::Constraints::INFINITY),
        );
        let line = density.sp_to_px(self.line_height).max(1.0);
        let lines = (text.height as f32 / line).round().max(1.0);
        let minimum = px(if lines > 1.0 { 68.0 } else { 48.0 });
        let height = constraints.constrain_height(
            minimum
                .max(text.height + 2 * vertical)
                .max(buttons.iter().map(|button| button.height).max().unwrap_or(0)),
        );
        scope.place_relative(&text, start, (height - text.height) / 2);
        let mut x = width - end - buttons_width;
        for button in &buttons {
            scope.place_relative(button, x, (height - button.height) / 2);
            x += button.width;
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "Snackbar"
    }
}

/// A snackbar with its action under its message, as Compose's `NewLineButtonSnackbar`: the
/// message 16 dp from the start and the end, 14 dp above and below; under it at the end, the
/// action then the dismiss button, 4 dp above the bottom (and 8 dp from the end without the
/// dismiss button); as wide as allowed up to 600 dp.
struct NewLineSnackbarLayout {
    dismiss: bool,
}

impl compose_ui::MeasurePolicy for NewLineSnackbarLayout {
    fn measure(
        &self,
        scope: &mut compose_ui::MeasureScope<'_>,
        measurables: &[compose_ui::Measurable],
        constraints: compose_ui::Constraints,
    ) -> compose_ui::MeasureResult {
        let density = scope.density();
        let px = |dp: f32| density.round_to_px(Dp(dp));
        let width = constraints.max_width.min(px(600.0)).max(constraints.min_width);
        let (start, vertical) = (px(16.0), px(14.0));
        let text = scope.measure(
            measurables[0],
            compose_ui::Constraints::new(
                0,
                (width - 2 * start).max(0),
                0,
                compose_ui::Constraints::INFINITY,
            ),
        );
        let loose = constraints.loosen();
        let buttons: Vec<_> =
            measurables[1..].iter().map(|&button| scope.measure(button, loose)).collect();
        let buttons_width: i32 = buttons.iter().map(|button| button.width).sum();
        let row_height = buttons.iter().map(|button| button.height).max().unwrap_or(0);
        let end = if self.dismiss { 0 } else { px(8.0) };
        let top = vertical + text.height + vertical;
        let height = constraints.constrain_height(top + row_height + px(4.0));
        scope.place_relative(&text, start, vertical);
        let mut x = width - end - buttons_width;
        for button in &buttons {
            scope.place_relative(button, x, top + (row_height - button.height) / 2);
            x += button.width;
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "NewLineButtonSnackbar"
    }
}

/// `text` in the text style and content color around it, as `Text(text)`.
fn themed_text(text: SharedString) {
    let style = compose_ui::TextStyle {
        color: LOCAL_TEXT_STYLE.current().color.or(LOCAL_CONTENT_COLOR.current()),
        ..LOCAL_TEXT_STYLE.current()
    };
    compose_foundation::BasicText(
        text,
        Modifier,
        style,
        None,
        compose_ui::TextOverflow::Clip,
        true,
        u32::MAX,
        1,
    );
}

/// A tooltip's look, as Compose's `PlainTooltip`: its content in the small body text on the
/// inverse surface, 8 dp across and 4 dp down from its edges, 40 to 200 dp wide, at least
/// 24 dp high. Show it with foundation's `TooltipArea`.
#[composable]
#[allow(non_snake_case)]
pub fn PlainTooltip(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::MaterialTheme::color_scheme().inverse_on_surface)] content_color: Color,
    #[default(crate::MaterialTheme::color_scheme().inverse_surface)] container_color: Color,
    #[default(crate::MaterialTheme::shapes().extra_small.clone())] shape: Shape,
    content: impl Fn() + 'static,
) {
    let typography = MaterialTheme::typography();
    Surface(
        modifier,
        shape,
        container_color,
        content_color,
        Dp(0.0),
        Dp(0.0),
        None,
        compose_runtime::callback(move || {
            let content = content.clone();
            CompositionLocalProvider(
                LOCAL_TEXT_STYLE.provides(typography.body_small.clone()),
                || {
                    let content = content.clone();
                    Box(
                        Modifier
                            .size_in(SizeSpec::bounds(40.dp(), 24.dp(), 200.dp(), Dp::UNSPECIFIED))
                            .padding((8.dp(), 4.dp())),
                        Alignment::CENTER_START,
                        false,
                        compose_runtime::callback(move |_| content()),
                    );
                },
            );
        }),
    );
}
