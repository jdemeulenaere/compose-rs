//! Composition locals read in content composed apart from where it's declared: lazy list items
//! (composed as they scroll in, and between frames when prefetched), dialogs and popups (drawn
//! over the window), and `BoxWithConstraints`' content (composed again once measured). They see
//! the providers around them, and a provider's new value recomposes them (D310: a read walks
//! provider groups only).

#![allow(non_snake_case)]

use std::cell::Cell;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Dialog, IntSize, Popup, UnitExt};

static LOCAL_LABEL: CompositionLocal<&'static str> =
    CompositionLocal::new("locals::LocalLabel", || "default");

thread_local! {
    static LABEL: Cell<Option<MutableState<&'static str>>> = const { Cell::new(None) };
    static TICKS: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
}

/// Shows the label provided around it (and `tick`, to run again on its own).
#[composable]
fn Labelled(name: String, tick: MutableState<u32>) {
    let label = LOCAL_LABEL.current();
    BasicText!("{name}: {label} ({})", tick.get());
}

/// `content` under a provider of a label in state, with an unrelated provider between.
fn provided(content: impl Fn(MutableState<u32>) + 'static) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let label = remember(|| mutable_state_of("first"));
        let tick = remember(|| mutable_state_of(0u32));
        LABEL.set(Some(label));
        TICKS.set(Some(tick));
        CompositionLocalProvider(LOCAL_LABEL.provides(label.get()), || {
            CompositionLocalProvider(
                compose_ui::LOCAL_BACK_HANDLERS_ENABLED.provides(true),
                || content(tick),
            );
        });
    })
}

fn set_label(ui: &mut ComposeTestRule, label: &'static str) {
    LABEL.get().unwrap().set(label);
    ui.wait_for_idle();
}

fn tick(ui: &mut ComposeTestRule) -> u32 {
    let ticks = TICKS.get().unwrap();
    ticks.update(|tick| tick + 1);
    ui.wait_for_idle();
    ticks.get()
}

#[test]
fn lazy_items_see_the_providers_around_the_list() {
    thread_local! {
        static LIST: Cell<Option<LazyListState>> = const { Cell::new(None) };
    }
    let state = LazyListState::new(0, 0);
    let given = *state;
    LIST.set(Some(given));
    let mut ui = provided(move |tick| {
        LazyColumn!(
            modifier = Modifier.size((200.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, move |index| {
                    Box!(modifier = Modifier.size((200.dp(), 20.dp())), {
                        Labelled(format!("item {index}"), tick);
                    });
                });
            }
        );
    });
    ui.on_node_with_text("item 0: first (0)").assert_exists();
    set_label(&mut ui, "second");
    ui.on_node_with_text("item 0: second (0)").assert_exists();
    ui.on_node_with_text("item 4: second (0)").assert_exists();
    // Items scrolled in compose under the providers too.
    LIST.get().unwrap().scroll_to_item(500, 0);
    ui.wait_for_idle();
    ui.on_node_with_text("item 500: second (0)").assert_exists();
    // Prefetched between frames, then shown.
    ui.on_node_with_text("item 501: second (0)").assert_exists();
    while ui.host().prefetch(std::time::Instant::now() + std::time::Duration::from_secs(1)) {}
    state.scroll_by(200.0);
    ui.wait_for_idle();
    ui.on_node_with_text("item 510: second (0)").assert_exists();
    // An item running again on its own still finds them.
    let tick = tick(&mut ui);
    ui.on_node_with_text(&format!("item 510: second ({tick})")).assert_exists();
    set_label(&mut ui, "third");
    ui.on_node_with_text(&format!("item 511: third ({tick})")).assert_exists();
}

#[test]
fn dialogs_and_popups_see_the_providers_around_them() {
    let mut ui = provided(|tick| {
        Labelled(String::from("screen"), tick);
        Dialog!(on_dismiss_request = || {}, {
            Labelled(String::from("dialog"), tick);
        });
        Popup!(on_dismiss_request = || {}, {
            Labelled(String::from("popup"), tick);
        });
    });
    for name in ["screen", "dialog", "popup"] {
        ui.on_node_with_text(&format!("{name}: first (0)")).assert_exists();
    }
    set_label(&mut ui, "second");
    let tick = tick(&mut ui);
    for name in ["screen", "dialog", "popup"] {
        ui.on_node_with_text(&format!("{name}: second ({tick})")).assert_exists();
    }
}

#[test]
fn content_composed_again_once_measured_sees_the_providers_around_it() {
    let mut ui = provided(|tick| {
        BoxWithConstraints!(
            modifier = Modifier.size((150.dp(), 50.dp())),
            |scope: BoxWithConstraintsScope| {
                Labelled(format!("box {}", scope.max_width().0 as i32), tick);
            }
        );
    });
    ui.on_node_with_text("box 150: first (0)").assert_exists();
    set_label(&mut ui, "second");
    let tick = tick(&mut ui);
    ui.on_node_with_text(&format!("box 150: second ({tick})")).assert_exists();
}
