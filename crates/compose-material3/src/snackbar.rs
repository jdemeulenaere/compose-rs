//! `Snackbar`, `SnackbarHost` and `SnackbarHostState`, as Compose's Material 3: brief messages
//! at the bottom of the screen, shown one at a time by `show_snackbar` (a coroutine returning
//! whether the action was performed), each for its duration.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use compose_animation::{AnimatedVisibility, fade_in_with, fade_out_with, scale_in, tween};
use compose_foundation::layout::{Arrangement, Box, Row, RowScope};
use compose_runtime::{
    ComposeArg, CompositionLocalProvider, LaunchedEffect, OwnedState, composable, remember,
};
use compose_ui::{Alignment, Color, Dp, Modifier, Shape, SizeSpec, UnitExt};

use crate::components::{Surface, TextButton};
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
    pub message: String,
    pub action_label: Option<String>,
    pub with_dismiss_action: bool,
    pub duration: SnackbarDuration,
}

/// A snackbar being shown, as Compose's `SnackbarData`: its visuals, and how to end it.
#[derive(Clone)]
pub struct SnackbarData {
    pub visuals: SnackbarVisuals,
    done: Rc<Completion>,
}

impl SnackbarData {
    /// Ends it as performed: `show_snackbar` returns `ActionPerformed`.
    pub fn perform_action(&self) {
        self.done.complete(SnackbarResult::ActionPerformed);
    }

    /// Ends it as dismissed.
    pub fn dismiss(&self) {
        self.done.complete(SnackbarResult::Dismissed);
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
                self.0.wakers.borrow_mut().push(context.waker().clone());
                Poll::Pending
            }
        }
    }
}

/// The snackbars to show, one at a time, as Compose's `SnackbarHostState`; remember one
/// (`remember(SnackbarHostState::new)`) and give it to a `SnackbarHost`.
#[derive(Clone)]
pub struct SnackbarHostState {
    inner: Rc<HostInner>,
}

struct HostInner {
    current: OwnedState<Option<SnackbarData>>,
    /// The last snackbar asked for: the next one waits for it to end.
    tail: RefCell<Option<Rc<Completion>>>,
}

impl Default for SnackbarHostState {
    fn default() -> Self {
        SnackbarHostState::new()
    }
}

impl PartialEq for SnackbarHostState {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl ComposeArg for SnackbarHostState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl SnackbarHostState {
    pub fn new() -> SnackbarHostState {
        SnackbarHostState {
            inner: Rc::new(HostInner {
                current: compose_runtime::mutable_state_of(None),
                tail: RefCell::new(None),
            }),
        }
    }

    /// The snackbar showing, if any, as `currentSnackbarData` (a state read).
    pub fn current_snackbar_data(&self) -> Option<SnackbarData> {
        self.inner.current.get()
    }

    /// Shows a snackbar once those asked for before it ended, and waits for it to end, as
    /// `showSnackbar`: `ActionPerformed` if its action was clicked. Cancelling the coroutine
    /// takes it away.
    pub async fn show_snackbar(
        &self,
        message: impl Into<String>,
        action_label: Option<String>,
        with_dismiss_action: bool,
        duration: SnackbarDuration,
    ) -> SnackbarResult {
        let visuals = SnackbarVisuals {
            message: message.into(),
            action_label,
            with_dismiss_action,
            duration,
        };
        let done = Rc::new(Completion::default());
        let previous = self.inner.tail.replace(Some(done.clone()));
        // Ends it however this coroutine ends: the next ones go on.
        let guard = Guard { host: self.inner.clone(), done: done.clone() };
        if let Some(previous) = previous {
            previous.wait().await;
        }
        let data = SnackbarData { visuals, done: done.clone() };
        self.inner.current.set(Some(data));
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
        let last =
            self.host.tail.borrow().as_ref().is_some_and(|tail| Rc::ptr_eq(tail, &self.done));
        if last {
            self.host.tail.replace(None);
        }
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
                compose_runtime::executor::delay(std::time::Duration::from_millis(millis)).await;
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
    AnimatedVisibility(
        current.is_some(),
        modifier,
        fade_in_with(tween(150), 0.0) + scale_in(0.8),
        fade_out_with(tween(75), 0.0),
        "SnackbarHost",
        compose_runtime::callback(move || {
            let Some(data) = shown.clone() else { return };
            match snackbar.clone() {
                Some(snackbar) => snackbar(data),
                None => Snackbar(
                    data,
                    Modifier,
                    MaterialTheme::shapes().extra_small,
                    MaterialTheme::color_scheme().inverse_surface,
                    MaterialTheme::color_scheme().inverse_on_surface,
                    MaterialTheme::color_scheme().inverse_primary,
                ),
            }
        }),
    );
}

/// A snackbar, as Compose's `Snackbar(snackbarData, …)`: its message (body-medium) and
/// optional action and dismiss button on the inverse surface, at least 48 dp high, with a 6 dp
/// shadow.
#[composable]
#[allow(non_snake_case)]
pub fn Snackbar(
    snackbar_data: SnackbarData,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::MaterialTheme::shapes().extra_small)] shape: Shape,
    #[default(crate::MaterialTheme::color_scheme().inverse_surface)] container_color: Color,
    #[default(crate::MaterialTheme::color_scheme().inverse_on_surface)] content_color: Color,
    #[default(crate::MaterialTheme::color_scheme().inverse_primary)] action_color: Color,
) {
    let typography = MaterialTheme::typography();
    let visuals = snackbar_data.visuals.clone();
    let has_buttons = visuals.action_label.is_some() || visuals.with_dismiss_action;
    // 12 dp around it, as Compose's `Snackbar(snackbarData)`.
    Surface(
        modifier.padding(12.dp()),
        shape,
        container_color,
        content_color,
        Dp(0.0),
        6.dp(),
        None,
        compose_runtime::callback(move || {
            let (data, visuals) = (snackbar_data.clone(), visuals.clone());
            CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(typography.body_medium), || {
                let (data, visuals) = (data.clone(), visuals.clone());
                Row(
                    Modifier
                        .size_in(SizeSpec::bounds(
                            Dp::UNSPECIFIED,
                            48.dp(),
                            Dp::UNSPECIFIED,
                            Dp::UNSPECIFIED,
                        ))
                        .padding_sides(
                            16.dp(),
                            0.dp(),
                            if has_buttons { 8.dp() } else { 16.dp() },
                            0.dp(),
                        ),
                    Arrangement::Start,
                    Alignment::CENTER_VERTICALLY,
                    compose_runtime::callback(move |row: RowScope| {
                        let message = visuals.message.clone();
                        Box(
                            Modifier.weight(row, 1.0).padding_horizontal_vertical(0.dp(), 6.dp()),
                            Alignment::CENTER_START,
                            false,
                            compose_runtime::callback(move |_| themed_text(message.clone())),
                        );
                        if let Some(label) = visuals.action_label.clone() {
                            let data = data.clone();
                            CompositionLocalProvider(
                                LOCAL_CONTENT_COLOR.provides(action_color),
                                || {
                                    let (data, label) = (data.clone(), label.clone());
                                    TextButton(
                                        compose_runtime::callback(move || data.perform_action()),
                                        Modifier,
                                        true,
                                        compose_runtime::callback(move |_| {
                                            themed_text(label.clone())
                                        }),
                                    );
                                },
                            );
                        }
                        if visuals.with_dismiss_action {
                            let data = data.clone();
                            IconButton(
                                compose_runtime::callback(move || data.dismiss()),
                                Modifier,
                                true,
                                compose_runtime::callback(|| {
                                    Icon(
                                        icons::filled::close(),
                                        Some("Dismiss"),
                                        Modifier,
                                        LOCAL_CONTENT_COLOR.current(),
                                    )
                                }),
                            );
                        }
                    }),
                );
            });
        }),
    );
}

/// `text` in the text style and content color around it, as `Text(text)`.
fn themed_text(text: String) {
    let style = compose_ui::TextStyle {
        color: LOCAL_TEXT_STYLE.current().color.or(LOCAL_CONTENT_COLOR.current()),
        ..LOCAL_TEXT_STYLE.current()
    };
    compose_foundation::BasicText(
        text.into(),
        Modifier,
        style,
        compose_ui::TextOverflow::Clip,
        true,
        u32::MAX,
    );
}
