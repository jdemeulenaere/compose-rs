//! Compose's `TooltipTest.kt`, adapted: `TooltipState::show` from a scope kept by the test, and a
//! trackpad is a mouse here (`perform_mouse_input`).

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::{MutatorMutex, ScrollModifiers as _, remember_scroll_state};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    FocusRequester, IntOffset, IntSize, Key, LayoutDirection, Modifier, Offset,
    PopupPositionProvider, Rect, SemanticsActions, UnitExt,
};

const ACTION: &str = "Action";
const ANCHOR: &str = "Anchor";
const CONTAINER: &str = "Container";
const SUBHEAD: &str = "Subhead";
const TEXT: &str = "Text";
const FADE: u64 = 300;
const DURATION: u64 = 1500;

thread_local! {
    static SCOPE: Cell<Option<CoroutineScope>> = const { Cell::new(None) };
}

fn show(state: TooltipState) {
    SCOPE.with(Cell::get).expect("composed").launch(async move { state.show().await });
}

fn advance(ui: &mut ComposeTestRule, millis: u64) {
    ui.advance_time_by(Duration::from_millis(millis));
}

/// `content` in a theme at the window's top start, with a scope for `show`, and a manual clock.
fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    let content = Rc::new(content);
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let content = content.clone();
        MaterialTheme!({
            SCOPE.with(|scope| scope.set(Some(remember_coroutine_scope())));
            let content = content.clone();
            Box!(modifier = Modifier.padding(200.dp()), {
                content();
            });
        });
    });
    ui.set_auto_advance(false);
    ui
}

fn above() -> Rc<dyn PopupPositionProvider> {
    TooltipDefaults::remember_tooltip_position_provider(TooltipAnchorPosition::Above)
}

#[composable]
fn PlainBox(
    state: TooltipState,
    #[default(compose_ui::Modifier)] modifier: Modifier,
    tooltip_content: impl Fn() + 'static,
) {
    TooltipBox!(
        above(),
        move |_: TooltipScope| {
            let tooltip_content = tooltip_content.clone();
            PlainTooltip!(
                modifier = modifier.clone().test_tag(CONTAINER),
                content = move || tooltip_content()
            );
        },
        state,
        {
            Icon!(icons::filled::favorite(), None);
        }
    );
}

#[composable]
fn RichBox(
    state: TooltipState,
    #[default(compose_ui::Modifier)] modifier: Modifier,
    #[default(None)] title: Option<impl Fn() + 'static>,
    #[default(None)] action: Option<impl Fn() + 'static>,
    text: impl Fn() + 'static,
) {
    TooltipBox!(
        above(),
        move |_: TooltipScope| {
            let (title, action, text) = (title.clone(), action.clone(), text.clone());
            RichTooltip!(
                modifier = modifier.clone().test_tag(CONTAINER),
                title = title,
                action = action,
                text = move || text()
            );
        },
        state,
        {
            Icon!(icons::filled::favorite(), None);
        }
    );
}

fn kept(persistent: bool) -> (Owned<TooltipState>, TooltipState) {
    let owned = TooltipState::new(false, persistent, MutatorMutex::default());
    let state = *owned;
    (owned, state)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltip_noContent_size
#[test]
fn an_empty_plain_tooltip_is_its_minimum_size() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        PlainBox!(state, tooltip_content = || {});
    });
    show(state);
    advance(&mut ui, FADE);
    ui.on_node_with_tag(CONTAINER)
        .assert_height_is_equal_to(24.dp())
        .assert_width_is_equal_to(40.dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, richTooltip_noContent_size
#[test]
fn an_empty_rich_tooltip_is_its_minimum_size() {
    let (_owned, state) = kept(true);
    let mut ui = rule(move || {
        RichBox!(state, text = || {});
    });
    show(state);
    advance(&mut ui, FADE);
    ui.on_node_with_tag(CONTAINER)
        .assert_height_is_equal_to(24.dp())
        .assert_width_is_equal_to(40.dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltip_customSize_size
#[test]
fn a_plain_tooltip_takes_a_size() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        PlainBox!(state, modifier = Modifier.size((100.dp(), 100.dp())), tooltip_content = || {});
    });
    show(state);
    advance(&mut ui, FADE);
    ui.on_node_with_tag(CONTAINER)
        .assert_height_is_equal_to(100.dp())
        .assert_width_is_equal_to(100.dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, richTooltip_customSize_size
#[test]
fn a_rich_tooltip_takes_a_size() {
    let (_owned, state) = kept(true);
    let mut ui = rule(move || {
        RichBox!(state, modifier = Modifier.size((100.dp(), 100.dp())), text = || {});
    });
    show(state);
    advance(&mut ui, FADE);
    ui.on_node_with_tag(CONTAINER)
        .assert_height_is_equal_to(100.dp())
        .assert_width_is_equal_to(100.dp());
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag_unmerged(tag).fetch_semantics_snapshot().bounds
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltip_content_padding
#[test]
fn a_plain_tooltips_content_is_8_across_and_4_down() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        PlainBox!(
            state,
            tooltip_content = || {
                Text!("Test", modifier = Modifier.test_tag(TEXT));
            }
        );
    });
    show(state);
    advance(&mut ui, FADE);
    let (container, text) = (bounds(&mut ui, CONTAINER), bounds(&mut ui, TEXT));
    assert_eq!(text.left - container.left, 8.0);
    assert_eq!(text.top - container.top, 4.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, richTooltip_content_padding
#[test]
fn a_rich_tooltips_parts_are_at_their_baselines() {
    let (_owned, state) = kept(true);
    let mut ui = rule(move || {
        RichBox!(
            state,
            title = Some(|| Text!("Subhead", modifier = Modifier.test_tag(SUBHEAD))),
            action = Some(|| Text!("Action", modifier = Modifier.test_tag(ACTION))),
            text = || {
                Text!("Text", modifier = Modifier.test_tag(TEXT));
            }
        );
    });
    show(state);
    advance(&mut ui, FADE);
    let container = bounds(&mut ui, CONTAINER);
    let (subhead, text, action) =
        (bounds(&mut ui, SUBHEAD), bounds(&mut ui, TEXT), bounds(&mut ui, ACTION));
    assert_eq!(subhead.left - container.left, 16.0);
    assert_eq!(text.left - container.left, 16.0);
    assert_eq!(action.left - container.left, 16.0);
    let subhead_baseline = ui.on_node_with_tag_unmerged(SUBHEAD).get_first_baseline_position();
    let text_baseline = ui.on_node_with_tag_unmerged(TEXT).get_first_baseline_position();
    assert_eq!(subhead.top - container.top, 28.0 - subhead_baseline.0);
    assert_eq!(text.top - subhead.bottom, 24.0 - text_baseline.0);
    assert_eq!(action.top - text.bottom, 16.0);
}

/// The slot's own text says, not the style: a text of another size keeps its baseline 28 dp down.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Tooltip.kt, RichTooltip
#[test]
fn a_rich_tooltips_title_of_another_size_keeps_its_baseline_in_place() {
    let (_owned, state) = kept(true);
    let mut ui = rule(move || {
        RichBox!(
            state,
            title = Some(|| Text!(
                "Subhead",
                modifier = Modifier.test_tag(SUBHEAD),
                style = compose_ui::TextStyle {
                    font_size: compose_ui::TextUnit::sp(10.0),
                    line_height: compose_ui::TextUnit::sp(10.0),
                    ..compose_ui::TextStyle::DEFAULT
                }
            )),
            text = || {
                Text!("Text", modifier = Modifier.test_tag(TEXT));
            }
        );
    });
    show(state);
    advance(&mut ui, FADE);
    let (container, subhead) = (bounds(&mut ui, CONTAINER), bounds(&mut ui, SUBHEAD));
    let baseline = ui.on_node_with_tag_unmerged(SUBHEAD).get_first_baseline_position();
    assert_eq!(subhead.top - container.top + baseline.0, 28.0);
}

/// With neither a title nor an action, the text has 4 dp above and below, as a plain one.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Tooltip.kt, textVerticalPadding
#[test]
fn a_rich_tooltip_of_text_alone_is_padded_as_a_plain_one() {
    let (_owned, state) = kept(true);
    let mut ui = rule(move || {
        RichBox!(
            state,
            text = || {
                Text!("Text", modifier = Modifier.test_tag(TEXT));
            }
        );
    });
    show(state);
    advance(&mut ui, FADE);
    let (container, text) = (bounds(&mut ui, CONTAINER), bounds(&mut ui, TEXT));
    assert_eq!(text.top - container.top, 4.0);
    assert_eq!(container.bottom - text.bottom, 4.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltip_behavior
#[test]
fn a_plain_tooltip_leaves_after_its_duration() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        PlainBox!(
            state,
            tooltip_content = || {
                Text!("Test");
            }
        );
    });
    assert!(!state.is_visible());
    show(state);
    advance(&mut ui, FADE);
    assert!(state.is_visible());
    advance(&mut ui, DURATION);
    assert!(!state.is_visible());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, richTooltip_behavior_noAction
#[test]
fn a_rich_tooltip_not_persistent_leaves_after_its_duration() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        RichBox!(
            state,
            title = Some(|| Text!("Subhead")),
            text = || {
                Text!("Text");
            }
        );
    });
    assert!(!state.is_visible());
    show(state);
    advance(&mut ui, FADE);
    assert!(state.is_visible());
    advance(&mut ui, DURATION);
    assert!(!state.is_visible());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, richTooltip_behavior_persistent
#[test]
fn a_persistent_tooltip_stays_until_dismissed() {
    let (_owned, state) = kept(true);
    let mut ui = rule(move || {
        RichBox!(
            state,
            action = Some(move || {
                TextButton!(move || state.dismiss(), modifier = Modifier.test_tag(ACTION), {
                    Text!("Action");
                });
            }),
            text = || {
                Text!("Text");
            }
        );
    });
    show(state);
    advance(&mut ui, FADE);
    assert!(state.is_visible());
    advance(&mut ui, DURATION);
    assert!(state.is_visible());
    ui.on_node_with_tag(ACTION).perform_touch_input(|touch| touch.click());
    advance(&mut ui, FADE);
    assert!(!state.is_visible());
}

#[composable]
fn Tagged(state: TooltipState) {
    Box!(modifier = Modifier.test_tag("tooltip"), {
        PlainBox!(
            state,
            tooltip_content = || {
                Text!("Test");
            }
        );
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltip_longPress_showsTooltip
#[test]
fn a_long_press_shows_it() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || Tagged!(state));
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    advance(&mut ui, 300);
    assert!(!state.is_visible());
    advance(&mut ui, 200);
    assert!(state.is_visible());
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| touch.up());
    advance(&mut ui, FADE + FADE + 500 + DURATION);
    assert!(!state.is_visible());
}

/// A press lifted before the timeout shows nothing, and one that left the anchor neither.
#[test]
fn a_short_press_shows_nothing() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || Tagged!(state));
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
        touch.up();
    });
    advance(&mut ui, 1000);
    assert!(!state.is_visible());
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
        touch.move_by(Offset::new(200.0, 0.0));
    });
    advance(&mut ui, 1000);
    assert!(!state.is_visible());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltip_longPress_keepsTooltipVisible
#[test]
fn a_long_press_keeps_it_while_held() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || Tagged!(state));
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    advance(&mut ui, 10_000);
    assert!(state.is_visible());
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| touch.up());
    advance(&mut ui, FADE);
    assert!(!state.is_visible());
}

/// The release of a long press isn't the click of what's in the box.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/BasicTooltip.kt, handleGestures
#[test]
fn a_long_press_does_not_click_what_is_in_the_box() {
    let (_owned, state) = kept(false);
    let clicks = Rc::new(Cell::new(0));
    let clicked = clicks.clone();
    let mut ui = rule(move || {
        let clicked = clicked.clone();
        TooltipBox!(
            above(),
            |_: TooltipScope| {
                PlainTooltip!(content = || {});
            },
            state,
            {
                let clicked = clicked.clone();
                Box!(
                    modifier = Modifier
                        .size(50.dp())
                        .test_tag(ANCHOR)
                        .clickable(move || clicked.set(clicked.get() + 1))
                );
            }
        );
    });
    ui.on_node_with_tag(ANCHOR).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    advance(&mut ui, 500);
    assert!(state.is_visible());
    ui.on_node_with_tag(ANCHOR).perform_touch_input(|touch| touch.up());
    assert_eq!(clicks.get(), 0);
    advance(&mut ui, FADE);
    ui.on_node_with_tag(ANCHOR).perform_touch_input(|touch| touch.click());
    assert_eq!(clicks.get(), 1);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltip_initialIsVisible_True
#[test]
fn it_can_start_visible() {
    let owned = TooltipState::new(true, false, MutatorMutex::default());
    let state = *owned;
    let mut ui = rule(move || {
        TooltipBox!(
            above(),
            |_: TooltipScope| {
                PlainTooltip!(modifier = Modifier.test_tag(CONTAINER), content = || {});
            },
            state,
            {
                Box!(modifier = Modifier.size(30.dp()));
            }
        );
    });
    advance(&mut ui, 16);
    assert!(state.is_visible());
    ui.on_node_with_tag(CONTAINER).assert_is_displayed();
}

#[composable]
fn Focusable(state: TooltipState, focus: FocusRequester, outside: FocusRequester) {
    Column!({
        TooltipBox!(
            above(),
            |_: TooltipScope| {
                PlainTooltip!(content = || {});
            },
            state,
            {
                Box!(
                    modifier = Modifier
                        .size(30.dp())
                        .test_tag(ANCHOR)
                        .focus_requester(focus)
                        .focus_target()
                );
            }
        );
        Box!(modifier = Modifier.size(30.dp()).focus_requester(outside).focus_target());
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltip_keyboardFocus_showsTooltip, plainTooltip_keyboardUnfocus_dismissesTooltip
#[test]
fn the_keyboards_focus_shows_it_and_its_leaving_dismisses_it() {
    let (_owned, state) = kept(false);
    let (focus, outside) = (FocusRequester::new(), FocusRequester::new());
    let mut ui = rule(move || Focusable!(state, focus, outside));
    assert!(!state.is_visible());
    focus.request_focus();
    advance(&mut ui, 100);
    assert!(state.is_visible());
    outside.request_focus();
    advance(&mut ui, FADE);
    assert!(!state.is_visible());
}

/// It stays past the duration while the anchor has the focus.
#[test]
fn it_stays_while_the_anchor_has_the_focus() {
    let (_owned, state) = kept(false);
    let (focus, outside) = (FocusRequester::new(), FocusRequester::new());
    let mut ui = rule(move || Focusable!(state, focus, outside));
    focus.request_focus();
    advance(&mut ui, DURATION + 500);
    assert!(state.is_visible());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltip_escapeKey_dismissesTooltip
#[test]
fn escape_dismisses_it() {
    let (_owned, state) = kept(false);
    let (focus, outside) = (FocusRequester::new(), FocusRequester::new());
    let mut ui = rule(move || Focusable!(state, focus, outside));
    focus.request_focus();
    advance(&mut ui, 100);
    assert!(state.is_visible());
    ui.press_key(Key::Escape);
    advance(&mut ui, FADE);
    assert!(!state.is_visible());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, tooltip_persistentState_showsOnKeyboardFocusThenProgrammatically
#[test]
fn a_persistent_one_shows_by_focus_then_by_show() {
    let (_owned, state) = kept(true);
    let (focus, outside) = (FocusRequester::new(), FocusRequester::new());
    let mut ui = rule(move || Focusable!(state, focus, outside));
    focus.request_focus();
    advance(&mut ui, 100);
    assert!(state.is_visible());
    outside.request_focus();
    advance(&mut ui, FADE);
    assert!(!state.is_visible());
    show(state);
    advance(&mut ui, 100);
    assert!(state.is_visible());
}

fn provider_at(positioning: TooltipAnchorPosition) -> TooltipPositionProvider {
    TooltipPositionProvider::new(positioning, 4)
}

fn at(left: i32, top: i32, width: i32, height: i32) -> Rect {
    Rect::new(left as f32, top as f32, (left + width) as f32, (top + height) as f32)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltipPositioning_tooltipCollideWithTopOfScreen_flipToBelowAnchor, richTooltipPositioning_tooltipCollideWithTopOfScreen_flipToBelowAnchor
#[test]
fn colliding_with_the_top_it_flips_below_the_anchor() {
    let position = provider_at(TooltipAnchorPosition::Above).position(
        at(0, 0, 10, 20),
        IntSize::new(500, 1000),
        LayoutDirection::Ltr,
        IntSize::new(500, 500),
    );
    assert_eq!(position, IntOffset::new(0, 24));
}

/// Each position at home and flipped, kept in the window, as `TooltipPositionProviderImpl`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Tooltip.kt, TooltipPositionProviderImpl
#[test]
fn the_positions_and_their_flips() {
    use LayoutDirection::{Ltr, Rtl};
    use TooltipAnchorPosition::*;
    let window = IntSize::new(500, 500);
    let popup = IntSize::new(100, 50);
    let anchor = at(200, 200, 20, 20);
    let place = |positioning, anchor, direction| {
        provider_at(positioning).position(anchor, window, direction, popup)
    };
    assert_eq!(place(Above, anchor, Ltr), IntOffset::new(160, 146));
    assert_eq!(place(Below, anchor, Ltr), IntOffset::new(160, 224));
    assert_eq!(place(Left, anchor, Ltr), IntOffset::new(96, 185));
    assert_eq!(place(Right, anchor, Ltr), IntOffset::new(224, 185));
    assert_eq!(place(Start, anchor, Ltr), IntOffset::new(96, 185));
    assert_eq!(place(End, anchor, Ltr), IntOffset::new(224, 185));
    assert_eq!(place(Start, anchor, Rtl), IntOffset::new(224, 185));
    assert_eq!(place(End, anchor, Rtl), IntOffset::new(96, 185));
    // Flipped at the edges.
    assert_eq!(place(Below, at(200, 470, 20, 20), Ltr), IntOffset::new(160, 416));
    assert_eq!(place(Left, at(50, 200, 20, 20), Ltr), IntOffset::new(74, 185));
    assert_eq!(place(Right, at(450, 200, 20, 20), Ltr), IntOffset::new(346, 185));
    // Kept in the window.
    assert_eq!(place(Above, at(0, 200, 20, 20), Ltr), IntOffset::new(0, 146));
    assert_eq!(place(Above, at(480, 200, 20, 20), Ltr), IntOffset::new(400, 146));
    assert_eq!(place(Left, at(200, 0, 20, 20), Ltr), IntOffset::new(96, 0));
    assert_eq!(place(Left, at(200, 480, 20, 20), Ltr), IntOffset::new(96, 450));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltip_caretAnchorPositioning, richTooltip_caretAnchorPositioning
#[test]
fn its_scope_has_the_anchors_bounds() {
    let seen = Rc::new(Cell::new(None));
    let out = seen.clone();
    let owned = TooltipState::new(true, true, MutatorMutex::default());
    let state = *owned;
    let mut ui = rule(move || {
        let out = out.clone();
        TooltipBox!(
            above(),
            move |scope: TooltipScope| {
                let out = out.clone();
                PlainTooltip!(
                    modifier = Modifier.layout(move |measure, content, constraints| {
                        out.set(scope.obtain_anchor_bounds().map(|it| it.bounds_in_window()));
                        let placeable = measure.measure(content, constraints);
                        measure.place(&placeable, 0, 0);
                        measure.layout(placeable.width, placeable.height)
                    }),
                    content = || {}
                );
            },
            state,
            {
                Icon!(icons::filled::favorite(), None, modifier = Modifier.test_tag(ANCHOR));
            }
        );
    });
    advance(&mut ui, 100);
    assert_eq!(seen.get(), Some(bounds(&mut ui, ANCHOR)));
}

#[composable]
fn Two(top: TooltipState, bottom: TooltipState) {
    Column!({
        RichBox!(
            top,
            title = Some(|| Text!("Subhead")),
            text = || {
                Text!("Text");
            }
        );
        RichBox!(
            bottom,
            title = Some(|| Text!("Subhead")),
            text = || {
                Text!("Text");
            }
        );
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, tooltipSync_global_onlyOneVisible
#[test]
fn by_default_one_shows_at_a_time() {
    let top_owned = TooltipState::new(false, true, TooltipState::global_mutator_mutex());
    let bottom_owned = TooltipState::new(false, true, TooltipState::global_mutator_mutex());
    let (top, bottom) = (*top_owned, *bottom_owned);
    let mut ui = rule(move || Two!(top, bottom));
    show(top);
    show(bottom);
    advance(&mut ui, FADE);
    assert!(!top.is_visible());
    assert!(bottom.is_visible());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, tooltipSync_local_bothVisible
#[test]
fn with_their_own_mutexes_both_show() {
    let (_top_owned, top) = kept(true);
    let (_bottom_owned, bottom) = kept(true);
    let mut ui = rule(move || Two!(top, bottom));
    show(top);
    show(bottom);
    advance(&mut ui, FADE);
    assert!(top.is_visible());
    assert!(bottom.is_visible());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, tooltipMouseHover_notPersistentState_dismiss, tooltipTrackpadHover_notPersistentState_dismiss
#[test]
fn a_mouses_hover_shows_it_and_its_leaving_dismisses_it() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        TooltipBox!(above(), |_: TooltipScope| {}, state, {
            Box!(modifier = Modifier.size(30.dp()).test_tag(ANCHOR));
        });
    });
    advance(&mut ui, 16);
    assert!(!state.is_visible());
    ui.on_node_with_tag_unmerged(ANCHOR).perform_mouse_input(|mouse| {
        let center = mouse.center();
        mouse.move_to(center);
    });
    advance(&mut ui, FADE);
    assert!(state.is_visible());
    advance(&mut ui, DURATION);
    assert!(state.is_visible());
    ui.on_node_with_tag_unmerged(ANCHOR)
        .perform_mouse_input(|mouse| mouse.move_to(Offset::new(-1.0, -1.0)));
    advance(&mut ui, FADE);
    assert!(!state.is_visible());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, tooltipMouseHover_persistentState_persistent, tooltipTrackpadHover_persistentState_persistent
#[test]
fn a_persistent_one_stays_after_a_hover() {
    let (_owned, state) = kept(true);
    let mut ui = rule(move || {
        TooltipBox!(above(), |_: TooltipScope| {}, state, {
            Box!(modifier = Modifier.size(30.dp()).test_tag(ANCHOR));
        });
    });
    advance(&mut ui, 16);
    ui.on_node_with_tag_unmerged(ANCHOR).perform_mouse_input(|mouse| {
        let center = mouse.center();
        mouse.move_to(center);
    });
    advance(&mut ui, FADE);
    assert!(state.is_visible());
    ui.on_node_with_tag_unmerged(ANCHOR)
        .perform_mouse_input(|mouse| mouse.move_to(Offset::new(-1.0, -1.0)));
    advance(&mut ui, FADE);
    assert!(state.is_visible());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltip_withClickable_hasCorrectSemantics
#[test]
fn a_clickable_anchor_has_the_long_click_too() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        TooltipBox!(
            above(),
            |_: TooltipScope| {
                PlainTooltip!(
                    content = || {
                        Text!("Tooltip");
                    }
                );
            },
            state,
            {
                IconButton!(|| {}, modifier = Modifier.test_tag(ANCHOR), {
                    Icon!(icons::filled::favorite(), None);
                });
            }
        );
    });
    advance(&mut ui, 16);
    let node = ui.on_node_with_tag(ANCHOR).fetch_semantics_snapshot();
    assert!(node.config.contains(&SemanticsActions::ON_CLICK));
    assert!(node.config.contains(&SemanticsActions::ON_LONG_CLICK));
    // And the action shows it.
    ui.on_node_with_tag(ANCHOR).perform_semantics_action(&SemanticsActions::ON_LONG_CLICK);
    advance(&mut ui, 100);
    assert!(state.is_visible());
}

/// With no clickable inside, the box has it itself, as `ParentSemanticsNode`; with input off,
/// nothing has.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/ChildParentSemantics.kt, ParentSemanticsNode
#[test]
fn a_plain_anchor_has_the_long_click_on_the_box() {
    for enabled in [true, false] {
        let (_owned, state) = kept(false);
        let mut ui = rule(move || {
            TooltipBox!(
                above(),
                |_: TooltipScope| {},
                state,
                modifier = Modifier.test_tag("box"),
                enable_user_input = enabled,
                {
                    Box!(modifier = Modifier.size(30.dp()));
                }
            );
        });
        advance(&mut ui, 16);
        let node = ui.on_node_with_tag("box").fetch_semantics_snapshot();
        assert_eq!(node.config.contains(&SemanticsActions::ON_LONG_CLICK), enabled);
    }
}

/// With its input off, neither a press nor a hover shows it.
#[test]
fn with_its_input_off_only_show_shows_it() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        TooltipBox!(above(), |_: TooltipScope| {}, state, enable_user_input = false, {
            Box!(modifier = Modifier.size(30.dp()).test_tag(ANCHOR));
        });
    });
    advance(&mut ui, 16);
    ui.on_node_with_tag_unmerged(ANCHOR).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    advance(&mut ui, 600);
    assert!(!state.is_visible());
    ui.on_node_with_tag_unmerged(ANCHOR).perform_touch_input(|touch| touch.up());
    show(state);
    advance(&mut ui, 100);
    assert!(state.is_visible());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, fullWidthTooltipCaret_xPositioning
#[test]
fn a_full_width_tooltips_caret_is_at_the_anchors_middle() {
    let anchor = |left: f32, right: f32| Rect::new(left, 0.0, right, 0.0);
    assert_eq!(caret_x(1080.0, 1080, anchor(135.0, 185.0)), 160.0);
    assert_eq!(caret_x(1080.0, 1080, anchor(515.5, 565.5)), 540.5);
    assert_eq!(caret_x(1080.0, 1080, anchor(920.88, 970.88)), 945.88);
}

/// The other collisions of the caret.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Tooltip.kt, caretX
#[test]
fn a_caret_along_a_tooltip_at_the_screens_edges() {
    let anchor = |left: f32, right: f32| Rect::new(left, 0.0, right, 0.0);
    // Centered neatly: the tooltip's middle.
    assert_eq!(caret_x(200.0, 1080, anchor(500.0, 550.0)), 100.0);
    // Start aligned, by the left edge.
    assert_eq!(caret_x(200.0, 1080, anchor(10.0, 60.0)), 35.0 - 10.0);
    // End aligned, by the right edge.
    assert_eq!(caret_x(200.0, 1080, anchor(1020.0, 1070.0)), 1045.0 - 870.0);
}

/// The caret's points on a tooltip of 40 by 24 at a 30 dp anchor, placed by `positioning`: the
/// second operand of the outline's union (the platform draws it).
fn caret_points(positioning: TooltipAnchorPosition) -> Vec<(f32, f32)> {
    let owned = TooltipState::new(true, true, MutatorMutex::default());
    let state = *owned;
    let mut ui = rule(move || {
        let provider = TooltipDefaults::remember_tooltip_position_provider(positioning);
        TooltipBox!(
            provider,
            move |scope: TooltipScope| {
                PlainTooltip!(
                    scope = Some(scope),
                    caret_shape = Some(TooltipDefaults::caret_shape()),
                    content = || {}
                );
            },
            state,
            {
                Box!(modifier = Modifier.size(30.dp()));
            }
        );
    });
    advance(&mut ui, FADE);
    let ops = ui.host().display_list().ops.clone();
    let path = ops
        .iter()
        .find_map(|op| match op {
            compose_ui::platform::DrawOp::DrawPath { path, .. } => Some(path.clone()),
            _ => None,
        })
        .expect("drawn as a path");
    let caret = path
        .steps()
        .iter()
        .rev()
        .find_map(|step| match step {
            compose_ui::PathStep::Push(data) => Some(data),
            _ => None,
        })
        .expect("an operand");
    caret
        .commands()
        .iter()
        .filter_map(|command| match command {
            compose_ui::PathCommand::MoveTo(point) | compose_ui::PathCommand::LineTo(point) => {
                Some(((point.x * 100.0).round() / 100.0, (point.y * 100.0).round() / 100.0))
            }
            _ => None,
        })
        .collect()
}

/// At the anchor's middle above it, pointing down; a flip for below, a turn beside it.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Tooltip.kt, layoutCaret, TooltipCaretShape, DefaultTooltipCaretShape
#[test]
fn the_caret_points_at_the_anchor() {
    assert_eq!(
        caret_points(TooltipAnchorPosition::Above),
        [(20.0, 24.0), (28.0, 24.0), (20.0, 32.0), (12.0, 24.0)]
    );
    assert_eq!(
        caret_points(TooltipAnchorPosition::Below),
        [(20.0, 0.0), (28.0, 0.0), (20.0, -8.0), (12.0, 0.0)]
    );
    assert_eq!(
        caret_points(TooltipAnchorPosition::Left),
        [(40.0, 12.0), (40.0, 4.0), (48.0, 12.0), (40.0, 20.0)]
    );
    assert_eq!(
        caret_points(TooltipAnchorPosition::Right),
        [(0.0, 12.0), (0.0, 20.0), (-8.0, 12.0), (0.0, 4.0)]
    );
}

/// The default colors and shapes, by their tokens.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Tooltip.kt, TooltipDefaults
#[test]
fn the_defaults_are_the_tokens() {
    let _ui = rule(|| {
        let scheme = MaterialTheme::color_scheme();
        let shapes = MaterialTheme::shapes();
        assert_eq!(TooltipDefaults::plain_tooltip_container_color(), scheme.inverse_surface);
        assert_eq!(TooltipDefaults::plain_tooltip_content_color(), scheme.inverse_on_surface);
        assert_eq!(TooltipDefaults::plain_tooltip_container_shape(), shapes.extra_small);
        assert_eq!(TooltipDefaults::rich_tooltip_container_shape(), shapes.medium);
        let rich = TooltipDefaults::rich_tooltip_colors();
        assert_eq!(rich.container_color, scheme.surface_container);
        assert_eq!(rich.content_color, scheme.on_surface_variant);
        assert_eq!(rich.title_content_color, scheme.on_surface_variant);
        assert_eq!(rich.action_content_color, scheme.primary);
    });
    assert_eq!(TooltipDefaults::CARET_SIZE, compose_ui::DpSize::new(16.dp(), 8.dp()));
    assert_eq!(TooltipDefaults::PLAIN_TOOLTIP_MAX_WIDTH, 200.dp());
    assert_eq!(TooltipDefaults::RICH_TOOLTIP_MAX_WIDTH, 320.dp());
}

/// A press outside dismisses it, or asks `on_dismiss_request` if given.
#[test]
fn a_press_outside_dismisses_it_or_asks() {
    let owned = TooltipState::new(true, true, MutatorMutex::default());
    let state = *owned;
    let mut ui = rule(move || {
        TooltipBox!(above(), |_: TooltipScope| {}, state, {
            Box!(modifier = Modifier.size(30.dp()));
        });
    });
    advance(&mut ui, FADE);
    ui.on_root().perform_touch_input(|touch| touch.click_at(Offset::new(5.0, 5.0)));
    advance(&mut ui, FADE);
    assert!(!state.is_visible());
    let asked = Rc::new(Cell::new(0));
    let asking = asked.clone();
    let owned = TooltipState::new(true, true, MutatorMutex::default());
    let state = *owned;
    let mut ui = rule(move || {
        let asking = asking.clone();
        TooltipBox!(
            above(),
            |_: TooltipScope| {},
            state,
            on_dismiss_request = Some(move || asking.set(asking.get() + 1)),
            {
                Box!(modifier = Modifier.size(30.dp()));
            }
        );
    });
    advance(&mut ui, FADE);
    ui.on_root().perform_touch_input(|touch| touch.click_at(Offset::new(5.0, 5.0)));
    advance(&mut ui, FADE);
    assert_eq!(asked.get(), 1);
    assert!(state.is_visible());
}

/// Tab from the anchor goes to a tooltip with an action: the popup takes the keys from there.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/BasicTooltip.kt, keyboardBehavior
#[test]
fn tab_goes_to_a_tooltip_with_an_action() {
    for has_action in [true, false] {
        let (_owned, state) = kept(true);
        let focus = FocusRequester::new();
        let keys = Rc::new(Cell::new(0));
        let heard = keys.clone();
        let mut ui = rule(move || {
            let heard = heard.clone();
            TooltipBox!(
                above(),
                move |_: TooltipScope| {
                    RichTooltip!(
                        action = Some(move || {
                            TextButton!(|| {}, modifier = Modifier.test_tag(ACTION), {
                                Text!("Action");
                            });
                        }),
                        text = || {}
                    );
                },
                state,
                has_action = has_action,
                {
                    let heard = heard.clone();
                    Box!(
                        modifier = Modifier
                            .size(30.dp())
                            .on_key_event(move |_: compose_ui::KeyEvent| {
                                heard.set(heard.get() + 1);
                                false
                            })
                            .focus_requester(focus)
                            .focus_target()
                    );
                }
            );
        });
        focus.request_focus();
        advance(&mut ui, FADE);
        ui.press_key(Key::Tab);
        advance(&mut ui, 100);
        assert!(state.is_visible());
        let before = keys.get();
        ui.press_key(Key::Character('a'));
        advance(&mut ui, 16);
        // Down and up; a Tab's own up came to the anchor or not, whichever: not these.
        assert_eq!(keys.get() - before, if has_action { 0 } else { 2 }, "action {has_action}");
    }
}

/// It grows from 0.8 and fades in: a layer, gone once it's there.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Tooltip.kt, TooltipBox
#[test]
fn it_scales_and_fades_in_and_out() {
    use compose_ui::platform::DrawOp;
    let layer = |ui: &mut ComposeTestRule| {
        let ops = ui.host().display_list().ops.clone();
        let scale = ops.iter().find_map(|op| match op {
            DrawOp::Transform { scale, .. } if *scale != (1.0, 1.0) => Some(scale.0),
            _ => None,
        });
        let alpha = ops.iter().find_map(|op| match op {
            DrawOp::SaveLayerAlpha { alpha, .. } => Some(*alpha),
            _ => None,
        });
        (scale, alpha)
    };
    let (_owned, state) = kept(true);
    let mut ui = rule(move || {
        PlainBox!(state, modifier = Modifier.size(50.dp()), tooltip_content = || {});
    });
    show(state);
    advance(&mut ui, 64);
    let (scale, alpha) = layer(&mut ui);
    let (scale, alpha) = (scale.unwrap(), alpha.unwrap());
    assert!(scale > 0.8 && scale < 1.0, "{scale}");
    assert!(alpha > 0.0 && alpha < 1.0, "{alpha}");
    advance(&mut ui, 1000);
    assert_eq!(layer(&mut ui), (None, None));
    state.dismiss();
    advance(&mut ui, 64);
    let (scale, alpha) = layer(&mut ui);
    assert!(scale.unwrap() < 1.0 && alpha.unwrap() < 1.0);
    advance(&mut ui, 1000);
    ui.on_node_with_tag(CONTAINER).assert_does_not_exist();
}

/// What's in a tooltip has its color and text style, and a rich one its shadow.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Tooltip.kt, PlainTooltip, RichTooltip
#[test]
fn the_content_has_its_colors_and_styles() {
    let owned = TooltipState::new(true, true, MutatorMutex::default());
    let state = *owned;
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let out = seen.clone();
    let mut ui = rule(move || {
        let out = out.clone();
        TooltipBox!(
            above(),
            move |_: TooltipScope| {
                let (a, b, c, d) = (out.clone(), out.clone(), out.clone(), out.clone());
                PlainTooltip!(
                    content = move || {
                        a.borrow_mut()
                            .push((LOCAL_CONTENT_COLOR.current(), LOCAL_TEXT_STYLE.current()));
                    }
                );
                RichTooltip!(
                    title = Some(move || {
                        b.borrow_mut()
                            .push((LOCAL_CONTENT_COLOR.current(), LOCAL_TEXT_STYLE.current()));
                    }),
                    action = Some(move || {
                        c.borrow_mut()
                            .push((LOCAL_CONTENT_COLOR.current(), LOCAL_TEXT_STYLE.current()));
                    }),
                    text = move || {
                        d.borrow_mut()
                            .push((LOCAL_CONTENT_COLOR.current(), LOCAL_TEXT_STYLE.current()));
                    }
                );
            },
            state,
            {
                Box!(modifier = Modifier.size(30.dp()));
            }
        );
    });
    advance(&mut ui, FADE);
    let (scheme, type_scale) = ui.run_on_idle(|_| (light_color_scheme(), Typography::default()));
    let seen = seen.borrow();
    let last = &seen[seen.len() - 4..];
    assert_eq!(last[0], (scheme.inverse_on_surface, type_scale.body_small));
    assert_eq!(last[1], (scheme.on_surface_variant, type_scale.title_small));
    assert_eq!(last[2], (scheme.on_surface_variant, type_scale.body_medium));
    assert_eq!(last[3], (scheme.primary, type_scale.label_large));
    let dump = ui.dump_display_list();
    assert!(dump.contains("shadow"), "{dump}");
}

/// The box leaving ends a persistent `show`, as `onDispose`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/BasicTooltip.kt, BasicTooltipBox
#[test]
fn the_box_leaving_ends_show() {
    let (_owned, state) = kept(true);
    let flag = mutable_state_of(true);
    let shown = flag.handle();
    let mut ui = rule(move || {
        let _ = &flag;
        if shown.get() {
            PlainBox!(state, tooltip_content = || {});
        }
    });
    let ended = Rc::new(Cell::new(false));
    let ending = ended.clone();
    SCOPE.with(Cell::get).unwrap().launch(async move {
        state.show().await;
        ending.set(true);
    });
    advance(&mut ui, FADE);
    assert!(!ended.get());
    shown.set(false);
    advance(&mut ui, FADE);
    assert!(ended.get());
}

/// The popup says it's a tooltip.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/BasicTooltip.kt, TooltipPopup
#[test]
fn the_popup_has_a_pane_title_and_a_live_region() {
    use compose_ui::SemanticsProperties;
    let owned = TooltipState::new(true, true, MutatorMutex::default());
    let state = *owned;
    let mut ui = rule(move || {
        TooltipBox!(
            above(),
            |_: TooltipScope| {
                Box!(modifier = Modifier.size(30.dp()));
            },
            state,
            {
                Box!(modifier = Modifier.size(30.dp()));
            }
        );
    });
    advance(&mut ui, FADE);
    let node = ui
        .on_node(expect_value(&SemanticsProperties::PANE_TITLE, "tooltip".into()))
        .fetch_semantics_snapshot();
    assert_eq!(
        node.config.get(&SemanticsProperties::LIVE_REGION),
        Some(&compose_ui::LiveRegionMode::Assertive)
    );
}

/// With a clickable inside the long click goes on it, not on the box as well; with the clickable
/// gone, back on the box.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/ChildParentSemantics.kt, ChildSemanticsNode, ParentSemanticsNode
#[test]
fn the_long_click_is_on_the_clickable_while_there_is_one() {
    let (_owned, state) = kept(false);
    let flag = mutable_state_of(true);
    let clickable = flag.handle();
    let mut ui = rule(move || {
        let _ = &flag;
        TooltipBox!(above(), |_: TooltipScope| {}, state, modifier = Modifier.test_tag("box"), {
            if clickable.get() {
                IconButton!(|| {}, {
                    Icon!(icons::filled::favorite(), None);
                });
            } else {
                Box!(modifier = Modifier.size(30.dp()));
            }
        });
    });
    advance(&mut ui, 16);
    let long_click = |ui: &mut ComposeTestRule| {
        ui.on_node_with_tag_unmerged("box")
            .fetch_semantics_snapshot()
            .config
            .contains(&SemanticsActions::ON_LONG_CLICK)
    };
    assert!(!long_click(&mut ui));
    clickable.set(false);
    advance(&mut ui, 16);
    assert!(long_click(&mut ui));
}

#[composable]
fn Labelled(source: compose_ui::MutableInteractionSource, is_persistent: bool) {
    let source_of_box = source.clone();
    Label!(
        |_: TooltipScope| {
            PlainTooltip!(modifier = Modifier.test_tag(CONTAINER), content = || {});
        },
        interaction_source = Some(source),
        is_persistent = is_persistent,
        {
            Box!(
                modifier = Modifier.size(50.dp()).test_tag(ANCHOR).clickable_with(
                    compose_ui::ClickableOptions {
                        interaction_source: Some(source_of_box.clone()),
                        ..compose_ui::ClickableOptions::DEFAULT
                    },
                    || {},
                )
            );
        }
    );
}

/// A label shows while its source is pressed, gone at once; hover too.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Label.kt, HandleInteractions
#[test]
fn a_label_shows_while_pressed() {
    let source = compose_ui::MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = rule(move || Labelled!(given.clone(), false));
    advance(&mut ui, 16);
    ui.on_node_with_tag(CONTAINER).assert_does_not_exist();
    ui.on_node_with_tag(ANCHOR).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    advance(&mut ui, 200);
    ui.on_node_with_tag(CONTAINER).assert_exists();
    // Past the duration it's still there, a press being the user's.
    advance(&mut ui, DURATION + 100);
    ui.on_node_with_tag(CONTAINER).assert_exists();
    ui.on_node_with_tag(ANCHOR).perform_touch_input(|touch| touch.up());
    advance(&mut ui, 32);
    ui.on_node_with_tag(CONTAINER).assert_does_not_exist();
}

/// An exit while a drag goes on leaves it: the slider's thumb shrinking from under the pointer.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Label.kt, HandleInteractions
#[test]
fn a_label_stays_while_any_interaction_goes_on() {
    use compose_ui::{DragInteraction, DragStart, HoverEnter, HoverInteraction, Interaction};
    let source = compose_ui::MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = rule(move || Labelled!(given.clone(), false));
    advance(&mut ui, 16);
    let (enter, start) = (HoverEnter::new(), DragStart::new());
    source.try_emit(Interaction::Hover(HoverInteraction::Enter(enter)));
    source.try_emit(Interaction::Drag(DragInteraction::Start(start)));
    advance(&mut ui, 100);
    ui.on_node_with_tag(CONTAINER).assert_exists();
    source.try_emit(Interaction::Hover(HoverInteraction::Exit(enter)));
    advance(&mut ui, 100);
    ui.on_node_with_tag(CONTAINER).assert_exists();
    source.try_emit(Interaction::Drag(DragInteraction::Stop(start)));
    advance(&mut ui, 100);
    ui.on_node_with_tag(CONTAINER).assert_does_not_exist();
}

/// A persistent label is always there.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Label.kt, LabelStateImpl
#[test]
fn a_persistent_label_is_always_there() {
    let source = compose_ui::MutableInteractionSource::new();
    let mut ui = rule(move || Labelled!(source.clone(), true));
    advance(&mut ui, 16);
    ui.on_node_with_tag(CONTAINER).assert_exists();
    advance(&mut ui, DURATION * 2);
    ui.on_node_with_tag(CONTAINER).assert_exists();
}

/// A second `show` while one runs keeps it, for its full time: the first's end doesn't hide it.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Tooltip.kt, TooltipStateImpl
#[test]
fn a_second_show_keeps_it_for_its_own_time() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        PlainBox!(state, tooltip_content = || {});
    });
    show(state);
    advance(&mut ui, 500);
    assert!(state.is_visible());
    show(state);
    advance(&mut ui, 500);
    assert!(state.is_visible(), "the first's end hid it");
    // Past the first's 1.5 s, not the second's.
    advance(&mut ui, 900);
    assert!(state.is_visible(), "the first's time counted");
    advance(&mut ui, 600 + FADE);
    assert!(!state.is_visible());
}

/// In, out and in: the second hover shows it.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/BasicTooltip.kt, handleGestures
#[test]
fn a_second_hover_shows_it() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        TooltipBox!(above(), |_: TooltipScope| {}, state, {
            Box!(modifier = Modifier.size(30.dp()).test_tag(ANCHOR));
        });
    });
    advance(&mut ui, 16);
    for _ in 0..2 {
        ui.on_node_with_tag_unmerged(ANCHOR).perform_mouse_input(|mouse| {
            let center = mouse.center();
            mouse.move_to(center);
        });
        advance(&mut ui, FADE);
        assert!(state.is_visible());
        ui.on_node_with_tag_unmerged(ANCHOR)
            .perform_mouse_input(|mouse| mouse.move_to(Offset::new(-1.0, -1.0)));
        advance(&mut ui, FADE);
        assert!(!state.is_visible());
    }
    // In while it fades out, too.
    ui.on_node_with_tag_unmerged(ANCHOR).perform_mouse_input(|mouse| {
        let center = mouse.center();
        mouse.move_to(center);
    });
    advance(&mut ui, FADE);
    ui.on_node_with_tag_unmerged(ANCHOR)
        .perform_mouse_input(|mouse| mouse.move_to(Offset::new(-1.0, -1.0)));
    advance(&mut ui, 16);
    ui.on_node_with_tag_unmerged(ANCHOR).perform_mouse_input(|mouse| {
        let center = mouse.center();
        mouse.move_to(center);
    });
    advance(&mut ui, FADE);
    assert!(state.is_visible());
}

/// A label pressed twice shows both times.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Label.kt, HandleInteractions
#[test]
fn a_label_pressed_twice_shows_twice() {
    let source = compose_ui::MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = rule(move || Labelled!(given.clone(), false));
    advance(&mut ui, 16);
    for _ in 0..2 {
        ui.on_node_with_tag(ANCHOR).perform_touch_input(|touch| {
            let center = touch.center();
            touch.down(center);
        });
        advance(&mut ui, 200);
        ui.on_node_with_tag(CONTAINER).assert_exists();
        ui.on_node_with_tag(ANCHOR).perform_touch_input(|touch| touch.up());
        advance(&mut ui, 32);
        ui.on_node_with_tag(CONTAINER).assert_does_not_exist();
    }
}

/// A long press that then leaves the anchor ends as one lifted: gone with its time.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/BasicTooltip.kt, handleGestures
#[test]
fn a_long_press_leaving_the_anchor_ends_with_its_time() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || Tagged!(state));
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    advance(&mut ui, 500);
    assert!(state.is_visible());
    ui.on_node_with_tag("tooltip")
        .perform_touch_input(|touch| touch.move_by(Offset::new(0.0, 200.0)));
    advance(&mut ui, 100);
    assert!(state.is_visible(), "its time isn't over");
    advance(&mut ui, DURATION + FADE);
    assert!(!state.is_visible());
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| touch.up());
}

/// The same when a scroll took the moves after the long press.
#[test]
fn a_long_press_then_a_scroll_ends_with_its_time() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        Column!(modifier = Modifier.height(100.dp()).vertical_scroll(remember_scroll_state(0)), {
            Tagged!(state);
            Spacer!(Modifier.height(1000.dp()));
        });
    });
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    advance(&mut ui, 500);
    assert!(state.is_visible());
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| {
        for _ in 0..4 {
            touch.move_by(Offset::new(0.0, -5.0));
        }
    });
    advance(&mut ui, DURATION + FADE);
    assert!(!state.is_visible());
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| touch.up());
}

/// A slow drag a scroll takes is no long press, however long it's held.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/foundation/gestures/TapGestureDetector.kt, waitForUpOrCancellation
#[test]
fn a_slow_drag_a_scroll_took_shows_nothing() {
    let (_owned, state) = kept(false);
    let scroll = Rc::new(Cell::new(None));
    let out = scroll.clone();
    let mut ui = rule(move || {
        let page = remember_scroll_state(0);
        out.set(Some(page));
        Column!(modifier = Modifier.height(100.dp()).vertical_scroll(page), {
            Box!(modifier = Modifier.test_tag("tooltip"), {
                TooltipBox!(above(), |_: TooltipScope| {}, state, {
                    Box!(modifier = Modifier.size((200.dp(), 200.dp())));
                });
            });
            Spacer!(Modifier.height(1000.dp()));
        });
    });
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| {
        touch.down(Offset::new(100.0, 50.0));
    });
    advance(&mut ui, 100);
    // One move past the slop, so that the scroll takes it in the main pass: only the final pass
    // tells the anchor.
    ui.on_node_with_tag("tooltip")
        .perform_touch_input(|touch| touch.move_by(Offset::new(0.0, -25.0)));
    advance(&mut ui, 600);
    assert!(scroll.get().unwrap().value() > 0, "a scroll");
    assert!(!state.is_visible());
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| touch.up());
}

/// Near the bottom, it's at its anchor: the whole window counts, as `windowContainerSize`, as the
/// anchor does from the very top. Bars of 60 and 50 in 800, an anchor at 720 to 750.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Tooltip.kt, TooltipPositionProviderImpl
#[test]
fn near_the_bottom_it_stays_at_its_anchor_with_bars() {
    use compose_ui::platform::{Insets, PlatformWindowInsets};
    let owned = TooltipState::new(true, true, MutatorMutex::default());
    let state = *owned;
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        MaterialTheme!({
            Box!(modifier = Modifier.offset(100.dp(), 720.dp()).size(30.dp()), {
                TooltipBox!(
                    above(),
                    |_: TooltipScope| {
                        PlainTooltip!(modifier = Modifier.test_tag(CONTAINER), content = || {});
                    },
                    state,
                    {
                        Box!(modifier = Modifier.size(30.dp()));
                    }
                );
            });
        });
    });
    ui.host().set_window_insets(PlatformWindowInsets {
        status_bars: Insets { top: 60, ..Default::default() },
        navigation_bars: Insets { bottom: 50, ..Default::default() },
        ..Default::default()
    });
    ui.wait_for_idle();
    let tooltip = bounds(&mut ui, CONTAINER);
    assert_eq!(tooltip.bottom, 716.0, "{tooltip:?}");
}

/// Shift+Tab isn't a Tab to the tooltip: it leaves as it would.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/KeyboardUtil.kt, isTab
#[test]
fn shift_tab_does_not_go_to_the_tooltip() {
    let (_owned, state) = kept(true);
    let focus = FocusRequester::new();
    let keys = Rc::new(Cell::new(0));
    let heard = keys.clone();
    let mut ui = rule(move || {
        let heard = heard.clone();
        TooltipBox!(
            above(),
            |_: TooltipScope| {
                RichTooltip!(
                    action = Some(|| {
                        Text!("Action");
                    }),
                    text = || {}
                );
            },
            state,
            has_action = true,
            {
                let heard = heard.clone();
                Box!(
                    modifier = Modifier
                        .size(30.dp())
                        .on_key_event(move |_: compose_ui::KeyEvent| {
                            heard.set(heard.get() + 1);
                            false
                        })
                        .focus_requester(focus)
                        .focus_target()
                );
            }
        );
    });
    focus.request_focus();
    advance(&mut ui, FADE);
    let before = keys.get();
    let down = compose_ui::KeyEvent {
        key: Key::Tab,
        kind: compose_ui::KeyEventType::KeyDown,
        modifiers: compose_ui::KeyModifiers { shift: true, ..Default::default() },
    };
    ui.host().send_key(down);
    advance(&mut ui, 16);
    assert_eq!(keys.get() - before, 1, "the preview took it");
}

/// Still there at 1.4 s, gone after its 1.5 s and the fade.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TooltipTest.kt, plainTooltip_behavior
#[test]
fn it_shows_for_its_whole_duration() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        PlainBox!(state, tooltip_content = || {});
    });
    show(state);
    advance(&mut ui, 1400);
    assert!(state.is_visible());
}

/// A long press's own state: the time of a press before it doesn't end the next.
#[test]
fn each_press_has_its_own_time() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || Tagged!(state));
    let press = |ui: &mut ComposeTestRule| {
        ui.on_node_with_tag("tooltip").perform_touch_input(|touch| {
            let center = touch.center();
            touch.down(center);
        });
    };
    press(&mut ui);
    advance(&mut ui, 500);
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| touch.up());
    advance(&mut ui, 100);
    // Pressed again before the first's time is over, and held past it.
    press(&mut ui);
    advance(&mut ui, 500 + 1600);
    assert!(state.is_visible(), "held");
    ui.on_node_with_tag("tooltip").perform_touch_input(|touch| touch.up());
    advance(&mut ui, FADE);
    assert!(!state.is_visible());
}

/// A drawer item's a clickable as a `Surface` is: the long click of a tooltip around it goes on it.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/NavigationDrawer.kt, NavigationDrawerItem
#[test]
fn a_drawer_item_takes_the_long_click_too() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || {
        TooltipBox!(above(), |_: TooltipScope| {}, state, {
            NavigationDrawerItem!(
                { Text!("Inbox") },
                false,
                || {},
                modifier = Modifier.test_tag(ANCHOR)
            );
        });
    });
    advance(&mut ui, 16);
    let node = ui.on_node_with_tag(ANCHOR).fetch_semantics_snapshot();
    assert!(node.config.contains(&SemanticsActions::ON_LONG_CLICK));
}

/// A new `interaction_source` for a label: the new one's what it follows.
#[test]
fn a_label_follows_a_new_source() {
    let first = compose_ui::MutableInteractionSource::new();
    let second = compose_ui::MutableInteractionSource::new();
    let owned = mutable_state_of(first.clone());
    let source = owned.handle();
    let mut ui = rule(move || Labelled!(source.get(), false));
    advance(&mut ui, 16);
    source.set(second.clone());
    advance(&mut ui, 16);
    let enter = compose_ui::HoverEnter::new();
    second.try_emit(compose_ui::Interaction::Hover(compose_ui::HoverInteraction::Enter(enter)));
    advance(&mut ui, 100);
    ui.on_node_with_tag(CONTAINER).assert_exists();
}

/// What a press before knew of its `show` doesn't end the next's early.
#[test]
fn the_next_long_press_lasts_its_own_time() {
    let (_owned, state) = kept(false);
    let mut ui = rule(move || Tagged!(state));
    let down = |ui: &mut ComposeTestRule| {
        ui.on_node_with_tag("tooltip").perform_touch_input(|touch| {
            let center = touch.center();
            touch.down(center);
        });
    };
    let up = |ui: &mut ComposeTestRule| {
        ui.on_node_with_tag("tooltip").perform_touch_input(|touch| touch.up());
    };
    // Held past its time, lifted, gone.
    down(&mut ui);
    advance(&mut ui, 400 + DURATION + 100);
    up(&mut ui);
    advance(&mut ui, FADE);
    assert!(!state.is_visible());
    // Lifted well before its time: it stays for that.
    down(&mut ui);
    advance(&mut ui, 500);
    assert!(state.is_visible());
    up(&mut ui);
    advance(&mut ui, FADE);
    assert!(state.is_visible());
}

/// The action's at least 36 dp high, whatever's left for it, as `requiredHeightIn`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Tooltip.kt, RichTooltip
#[test]
fn the_action_has_its_height_in_a_short_tooltip() {
    let (_owned, state) = kept(true);
    let mut ui = rule(move || {
        RichBox!(
            state,
            modifier = Modifier.height(30.dp()),
            action = Some(|| {
                Box!(modifier = Modifier.width(10.dp()).fill_max_height().test_tag(ACTION));
            }),
            text = || {}
        );
    });
    show(state);
    advance(&mut ui, FADE);
    // Less the 8 dp below it.
    assert_eq!(bounds(&mut ui, ACTION).height(), 28.0);
}
