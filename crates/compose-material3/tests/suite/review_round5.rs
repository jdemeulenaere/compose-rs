//! Fifth review round: snackbars (queueing, layout, colors), text fields (label placement, the
//! outlined notch) and card elevation, against Jetpack Compose's material3. Each test states the
//! input, what Compose does, and what happens instead.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, Constraints, Density, DrawOp, FixedTextMeasurer, IntSize, Modifier, Offset,
    ParagraphHandle, ParagraphMetrics, TextLayoutOptions, TextMeasurer, TextPosition, TextStyle,
    UnitExt,
};

thread_local! {
    static RESULTS: RefCell<Vec<(String, SnackbarResult)>> = const { RefCell::new(Vec::new()) };
    static SCHEME: Cell<Option<ColorScheme>> = const { Cell::new(None) };
}

fn screen(host: SnackbarHostState, scope: Rc<RefCell<Option<CoroutineScope>>>) -> impl Fn() {
    move || {
        let coroutines = remember_coroutine_scope();
        *scope.borrow_mut() = Some(coroutines);
        MaterialTheme!({
            SCHEME.with(|cell| cell.set(Some(MaterialTheme::color_scheme())));
            Scaffold!(
                snackbar_host = move || SnackbarHost!(
                    host,
                    snackbar = Some(|data: SnackbarData| {
                        Snackbar!(data, modifier = Modifier.test_tag("bar"));
                    })
                ),
                |_padding| {}
            );
        });
    }
}

fn show(
    scope: &Rc<RefCell<Option<CoroutineScope>>>,
    host: &SnackbarHostState,
    message: &str,
    action: Option<&str>,
    duration: SnackbarDuration,
) -> Job {
    let (host, message, action) = (*host, message.to_string(), action.map(str::to_string));
    scope.borrow().as_ref().unwrap().launch(async move {
        let result = host.show_snackbar(message.clone(), action, false, duration).await;
        RESULTS.with(|results| results.borrow_mut().push((message, result)));
    })
}

fn results() -> Vec<(String, SnackbarResult)> {
    RESULTS.with(|results| results.borrow().clone())
}

// ---- Snackbar queue ---------------------------------------------------------------------------

/// Three `show_snackbar` calls queue: "First" shows, "Second" and "Third" wait. Cancelling
/// "Second" while it waits (its coroutine cancelled, as when the screen that asked for it
/// goes) must leave "First" showing and "Third" waiting: Compose's `showSnackbar` queues on a
/// `Mutex`, and a cancelled waiter just leaves the queue. Here each call waits on the
/// completion of the one asked for before it, and the cancelled "Second"'s `Guard` completes
/// that completion: "Third" wakes and replaces "First" at once. "First" is then no longer the
/// current snackbar, so the host's timeout (keyed on the current one) never dismisses it: its
/// `show_snackbar` never returns.
#[test]
fn cancelling_a_waiting_snackbar_does_not_let_the_next_one_jump_the_queue() {
    RESULTS.with(|results| results.borrow_mut().clear());
    let host = SnackbarHostState::new();
    let scope = Rc::new(RefCell::new(None));
    let mut ui = ComposeTestRule::with_manual_clock(screen(*host, scope.clone()));
    show(&scope, &host, "First", None, SnackbarDuration::Short);
    let second = show(&scope, &host, "Second", None, SnackbarDuration::Short);
    show(&scope, &host, "Third", None, SnackbarDuration::Short);
    ui.advance_time_by(Duration::from_millis(500));
    ui.on_node_with_text("First").assert_exists();
    second.cancel();
    ui.advance_time_by(Duration::from_millis(500));
    assert_eq!(
        host.current_snackbar_data().map(|data| data.visuals.message),
        Some("First".to_string()),
        "Compose: First keeps showing for its 4 s, Third waits"
    );
    // After First's 4 s, it's dismissed and Third shows.
    ui.advance_time_by(Duration::from_millis(3600));
    assert_eq!(results(), vec![("First".to_string(), SnackbarResult::Dismissed)]);
}

/// `show_snackbar` called from a `LaunchedEffect` (the usual pattern) that leaves the
/// composition while its snackbar shows: the coroutine is cancelled, the snackbar goes, and a
/// later one shows. Lock-in: cancelling from the composition's disposal writes the host's state.
#[test]
fn passes_a_snackbar_shown_from_an_effect_that_leaves_goes_with_it() {
    #[composable]
    fn Screen(host: SnackbarHostState, asking: MutableState<bool>) {
        MaterialTheme!({
            if asking.get() {
                LaunchedEffect((), async move {
                    host.show_snackbar("Asked", None, false, SnackbarDuration::Indefinite).await;
                });
            }
            Scaffold!(snackbar_host = move || SnackbarHost!(host), |_padding| {});
        });
    }
    let host = SnackbarHostState::new();
    let asking = mutable_state_of(true);
    let handle = asking.handle();
    let mut ui = ComposeTestRule::with_manual_clock({
        let host = *host;
        move || Screen(host, handle)
    });
    ui.advance_time_by(Duration::from_millis(500));
    ui.on_node_with_text("Asked").assert_exists();
    asking.set(false);
    ui.advance_time_by(Duration::from_millis(500));
    assert!(host.current_snackbar_data().is_none());
    ui.on_node_with_text("Asked").assert_does_not_exist();
}

// ---- Snackbar colors and layout ---------------------------------------------------------------

/// A text measurer recording each text laid out with its style's color.
struct Recording {
    inner: FixedTextMeasurer,
    log: Rc<RefCell<Vec<(String, Color)>>>,
}

impl TextMeasurer for Recording {
    fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        self.log.borrow_mut().push((text.to_string(), style.color));
        self.inner.layout(text, style, options, constraints, density)
    }
    fn metrics(&self, paragraph: ParagraphHandle) -> ParagraphMetrics {
        self.inner.metrics(paragraph)
    }
    fn hit_test(&self, paragraph: ParagraphHandle, position: Offset) -> TextPosition {
        self.inner.hit_test(paragraph, position)
    }
    fn cursor_rect(&self, paragraph: ParagraphHandle, offset: usize) -> compose_ui::Rect {
        self.inner.cursor_rect(paragraph, offset)
    }
    fn text(&self, paragraph: ParagraphHandle) -> &str {
        self.inner.text(paragraph)
    }
    fn release(&mut self, paragraph: ParagraphHandle) {
        self.inner.release(paragraph)
    }
}

/// Compose's `Snackbar(snackbarData)` draws its action as
/// `TextButton(colors = ButtonDefaults.textButtonColors(contentColor = actionColor))`, the
/// action color by default `inversePrimary` (light on the dark inverse surface). Here the action
/// color is provided as the content color around a `TextButton`, which provides its own
/// (`text_button_colors().content_color`, `primary`) over it: the "Undo" label is `primary`, a
/// dark purple on the dark inverse surface.
#[test]
fn a_snackbars_action_is_in_the_inverse_primary_color() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let host = SnackbarHostState::new();
    let scope = Rc::new(RefCell::new(None));
    let mut ui = compose_test_with(
        DEFAULT_SIZE,
        Density::default(),
        Box::new(Recording { inner: FixedTextMeasurer::new(), log: log.clone() }),
        screen(*host, scope.clone()),
    );
    show(&scope, &host, "Deleted", Some("Undo"), SnackbarDuration::Indefinite);
    ui.wait_for_idle();
    ui.on_node_with_text("Undo").assert_exists();
    let scheme = SCHEME.with(Cell::get).expect("composed");
    let color = log.borrow().iter().rev().find(|(text, _)| text == "Undo").map(|(_, color)| *color);
    assert_eq!(color, Some(scheme.inverse_primary), "primary is {:?}", scheme.primary);
}

/// Compose's snackbar is at most 600 dp wide (`ContainerMaxWidth`: `OneRowSnackbar` lays out at
/// `min(maxWidth, 600.dp)`), centered by the scaffold. Here it fills the window's width: in a
/// 1200 px window (density 1) it's 1176 px wide (with its 12 dp padding each side), where
/// Compose's is 624.
#[test]
fn a_snackbar_is_at_most_600_dp_wide() {
    let host = SnackbarHostState::new();
    let scope = Rc::new(RefCell::new(None));
    let mut ui = compose_test_with(
        IntSize::new(1200, 800),
        Density::default(),
        Box::new(FixedTextMeasurer::new()),
        screen(*host, scope.clone()),
    );
    show(&scope, &host, "Saved", None, SnackbarDuration::Indefinite);
    ui.wait_for_idle();
    let bar = ui.on_node_with_tag("bar").fetch_semantics_node().bounds;
    assert_eq!(bar.width(), 624.0, "600 dp and the 12 dp padding: {bar:?}");
    assert_eq!(bar.left, (1200.0 - 624.0) / 2.0, "centered");
}

/// Compose's one-row snackbar is at least 48 dp high on one line and 68 dp on two
/// (`SnackbarTokens.TwoLinesContainerHeight`, the message's first baseline 30 dp down). Here a
/// two-line message gets the one-line minimum and its own height with 6 dp above and below.
#[test]
fn a_two_line_snackbar_is_at_least_68_dp_high() {
    let host = SnackbarHostState::new();
    let scope = Rc::new(RefCell::new(None));
    let mut ui = compose_test_with(
        DEFAULT_SIZE,
        Density::default(),
        Box::new(FixedTextMeasurer::new()),
        screen(*host, scope.clone()),
    );
    let message = "This message is long enough that it wraps onto a second line";
    show(&scope, &host, message, None, SnackbarDuration::Indefinite);
    ui.wait_for_idle();
    let text = ui.on_node_with_text(message).fetch_semantics_node().bounds;
    let bar = ui.on_node_with_tag("bar").fetch_semantics_node().bounds;
    assert!(text.height() > 30.0, "two lines: {text:?}");
    let surface = bar.height() - 24.0;
    assert!(surface >= 68.0, "the snackbar's surface is {surface} px high");
}

// ---- Text fields ------------------------------------------------------------------------------

/// Compose's filled `TextField` (multi-line by default) rests its label at `TextFieldPadding`
/// (16 dp) from the top when it isn't single-line ("the label's start position is at the top
/// with padding"), where a line of text would be in a field without a label: centered in the
/// 56 dp field. Here it rests at the 8 dp top padding of a field with a label, where it floats
/// to: an empty, unfocused field shows its label high, off-center.
#[test]
fn a_multi_line_filled_fields_resting_label_is_16_dp_from_the_top() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_| {},
                modifier = Modifier.test_tag("field"),
                label = Some(|| Text!("Notes", modifier = Modifier.test_tag("label"))),
            );
        });
    });
    let field = ui.on_node_with_tag("field").fetch_semantics_node().bounds;
    let label = ui.on_node_unmerged(has_test_tag("label")).fetch_semantics_node().bounds;
    assert_eq!(label.height(), 24.0, "the large body style");
    assert_eq!(label.top - field.top, 16.0, "{field:?} {label:?}");
}

fn has_open_path(ui: &mut ComposeTestRule) -> bool {
    ui.host().display_list().ops.iter().any(|op| match op {
        DrawOp::DrawPath { path, .. } => {
            !matches!(path.commands().last(), Some(compose_ui::PathCommand::Close))
        }
        _ => false,
    })
}

/// An outlined field with text keeps its label floated, the outline cut around it, whatever
/// the text. Here `decorated_field` makes a new `label_width` cell at each composition and the
/// outline's draw reads the new one, but the measure policy is remembered (keyed on the
/// variant, the label, the placeholder, single-line and the progress), so after a
/// recomposition with the same key (typing a letter: the progress stays 1) the old policy
/// writes the old cell, the new cell stays 0, and the outline is drawn closed, through the
/// floated label.
#[test]
fn an_outlined_fields_outline_stays_cut_around_its_label_when_its_text_changes() {
    #[composable]
    fn Screen(value: MutableState<String>) {
        MaterialTheme!({
            OutlinedTextField!(
                value.get(),
                move |new| value.set(new),
                label = Some(|| Text!("City")),
                single_line = true,
            );
        });
    }
    let value = mutable_state_of(String::from("Paris"));
    let handle = value.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || Screen(handle));
    assert!(has_open_path(&mut ui), "cut at first: {}", ui.dump_display_list());
    value.set(String::from("Parisx"));
    ui.advance_time_by(Duration::from_millis(300));
    assert!(has_open_path(&mut ui), "still cut: {}", ui.dump_display_list());
}

// ---- Cards ------------------------------------------------------------------------------------

/// Compose's `ElevatedCard(onClick, enabled = false)` keeps its 1 dp shadow:
/// `CardDefaults.elevatedCardElevation()`'s `disabledElevation` is
/// `ElevatedCardTokens.DisabledContainerElevation`, level 1. Here a disabled card has no shadow.
#[test]
fn a_disabled_elevated_card_keeps_its_1_dp_shadow() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            ElevatedCard!(
                on_click = Some(|| {}),
                enabled = false,
                modifier = Modifier.size(40.dp()),
                {}
            );
        });
    });
    let shadows = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter(|op| matches!(op, DrawOp::DrawShadow { .. }))
        .count();
    assert_eq!(shadows, 1);
}

/// Compose's `OneRowSnackbar` pads its row 16 dp at the start and, at the end,
/// `if (dismissAction == null) HorizontalSpacingButtonSide (8 dp) else 0.dp`: the dismiss
/// button's 48 dp target touches the snackbar's end (its 40 dp circle 4 dp in). Here the end
/// padding is 8 dp whenever there's a button: the dismiss button sits 8 dp further in.
#[test]
fn a_snackbars_dismiss_button_is_at_its_end() {
    let host = SnackbarHostState::new();
    let scope = Rc::new(RefCell::new(None));
    let mut ui = compose_test(screen(*host, scope.clone()));
    {
        let host = *host;
        scope.borrow().as_ref().unwrap().launch(async move {
            host.show_snackbar("Saved", None, true, SnackbarDuration::Indefinite).await;
        });
    }
    ui.wait_for_idle();
    let bar = ui.on_node_with_tag("bar").fetch_semantics_node().bounds;
    let dismiss = ui.on_node_with_content_description("Dismiss").fetch_semantics_node().bounds;
    // The bar's 12 dp padding, no row padding, the 4 dp between the target and the circle.
    assert_eq!(bar.right - dismiss.right, 12.0 + 4.0, "{bar:?} {dismiss:?}");
}
