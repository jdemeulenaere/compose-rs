//! `SaveableStateHolder` (D290): the saveable state of content that leaves the composition,
//! kept by key; and the lazy list's items keeping theirs when scrolled away.

use std::cell::Cell;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::UiHost;
use compose_ui::*;

thread_local! {
    static SHOWN: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
    static HOLDER: Cell<Option<SaveableStateHolder>> = const { Cell::new(None) };
}

/// A screen with a saved counter and a remembered one, both clicked up.
#[composable]
fn Screen(name: u32) {
    let saved = remember_saveable(|| mutable_state_of(0u32));
    let plain = remember(|| mutable_state_of(0u32));
    Column!({
        Box!(
            modifier = Modifier
                .size(20.dp())
                .clickable(move || {
                    saved.update(|count| count + 1);
                    plain.update(|count| count + 1);
                })
                .test_tag(format!("screen {name}")),
        );
        BasicText!("{name}: saved {}, remembered {}", saved.get(), plain.get());
    });
}

/// One of three screens, each in a provider keyed by its number.
#[composable]
fn Screens() {
    let holder = remember_saveable_state_holder();
    HOLDER.set(Some(holder));
    let shown = remember_saveable(|| mutable_state_of(1u32));
    SHOWN.set(Some(shown));
    let name = shown.get();
    holder.SaveableStateProvider(name, || Screen(name));
}

fn show(ui: &mut ComposeTestRule, name: u32) {
    SHOWN.get().unwrap().set(name);
    ui.wait_for_idle();
}

/// The host is destroyed (an Android configuration change, the process's death) and a new one
/// composes the same content with what the first saved, through bytes.
fn recreate(mut ui: ComposeTestRule, content: impl Fn() + 'static) -> ComposeTestRule {
    let bytes = ui.host().save_state().to_bytes();
    drop(ui);
    let host = UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
    host.restore_state(SavedState::from_bytes(&bytes).expect("saved state bytes"));
    ComposeTestRule::with_host(host, content)
}

#[test]
fn content_that_comes_back_gets_its_saveable_state_back() {
    let mut ui = compose_test(Screens);
    ui.on_node_with_tag("screen 1").perform_click();
    ui.on_node_with_tag("screen 1").perform_click();
    ui.on_node_with_text("1: saved 2, remembered 2").assert_exists();
    show(&mut ui, 2);
    ui.on_node_with_tag("screen 2").perform_click();
    ui.on_node_with_text("2: saved 1, remembered 1").assert_exists();
    show(&mut ui, 1);
    // The saveable counter comes back; the remembered one went with the content, as in
    // Compose.
    ui.on_node_with_text("1: saved 2, remembered 0").assert_exists();
    show(&mut ui, 2);
    ui.on_node_with_text("2: saved 1, remembered 0").assert_exists();
}

#[test]
fn removed_state_is_not_given_back() {
    let mut ui = compose_test(Screens);
    ui.on_node_with_tag("screen 1").perform_click();
    show(&mut ui, 2);
    ui.on_node_with_tag("screen 2").perform_click();
    // A key not shown: what it kept goes.
    HOLDER.get().unwrap().remove_state(1u32);
    // The key shown: it keeps its state while it stays, and leaves none behind.
    HOLDER.get().unwrap().remove_state(2u32);
    ui.on_node_with_text("2: saved 1, remembered 1").assert_exists();
    show(&mut ui, 1);
    ui.on_node_with_text("1: saved 0, remembered 0").assert_exists();
    show(&mut ui, 2);
    ui.on_node_with_text("2: saved 0, remembered 0").assert_exists();
}

#[test]
fn the_holder_is_saved_with_what_it_keeps_across_recreation() {
    let mut ui = compose_test(Screens);
    ui.on_node_with_tag("screen 1").perform_click();
    show(&mut ui, 3);
    ui.on_node_with_tag("screen 3").perform_click();
    ui.on_node_with_tag("screen 3").perform_click();
    // Screen 3 in the composition, screen 1 kept by the holder: both are saved.
    let mut ui = recreate(ui, Screens);
    ui.on_node_with_text("3: saved 2, remembered 0").assert_exists();
    show(&mut ui, 1);
    ui.on_node_with_text("1: saved 1, remembered 0").assert_exists();
    // And saved again by the new host.
    let mut ui = recreate(ui, Screens);
    ui.on_node_with_text("1: saved 1, remembered 0").assert_exists();
    show(&mut ui, 3);
    ui.on_node_with_text("3: saved 2, remembered 0").assert_exists();
}

#[test]
fn content_coming_back_elsewhere_finds_its_state() {
    thread_local! { static PLACE: Cell<Option<MutableState<u8>>> = const { Cell::new(None) }; }
    // The provider under another parent when it comes back (as in an animated container's
    // next slot): the values are found by their place within the provider.
    let content = || {
        let holder = remember_saveable_state_holder();
        let place = remember(|| mutable_state_of(0u8));
        PLACE.set(Some(place));
        match place.get() {
            0 => holder.SaveableStateProvider("screen", || Screen(1)),
            1 => {}
            _ => Box!({
                Column!({
                    holder.SaveableStateProvider("screen", || Screen(1));
                });
            }),
        }
    };
    let mut ui = compose_test(content);
    ui.on_node_with_tag("screen 1").perform_click();
    PLACE.get().unwrap().set(1);
    ui.wait_for_idle();
    PLACE.get().unwrap().set(2);
    ui.wait_for_idle();
    ui.on_node_with_text("1: saved 1, remembered 0").assert_exists();
}

#[test]
fn a_provider_without_a_registry_above_still_keeps_state() {
    // The holder's own state isn't saved without a host registry, but it keeps its content's.
    let mut ui = compose_test(|| {
        CompositionLocalProvider(LOCAL_SAVEABLE_STATE_REGISTRY.provides(None), Screens);
    });
    ui.on_node_with_tag("screen 1").perform_click();
    show(&mut ui, 2);
    show(&mut ui, 1);
    ui.on_node_with_text("1: saved 1, remembered 0").assert_exists();
}

// ---- Lazy lists ------------------------------------------------------------------------------

thread_local! {
    static LIST: Cell<Option<LazyListState>> = const { Cell::new(None) };
}

/// A hundred items with a saveable counter each, 50 dp tall in a 200 dp list.
#[composable]
fn CountingList() {
    let state = remember_lazy_list_state!();
    LIST.set(Some(state));
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 200.dp())),
        state = state,
        |scope: LazyListScope| {
            scope.items_keyed(
                100,
                |index| index as u32,
                |index| {
                    let clicks = remember_saveable(|| mutable_state_of(0u32));
                    Box!(
                        modifier = Modifier
                            .size((100.dp(), 50.dp()))
                            .clickable(move || clicks.update(|count| count + 1))
                            .test_tag(format!("item {index}")),
                        {
                            BasicText!("item {index}: {}", clicks.get());
                        }
                    );
                },
            );
        }
    );
}

fn scroll_to(ui: &mut ComposeTestRule, index: usize) {
    LIST.get().unwrap().scroll_to_item(index, 0);
    ui.wait_for_idle();
}

#[test]
fn lazy_items_keep_their_saveable_state_when_scrolled_away() {
    let mut ui = compose_test(CountingList);
    ui.on_node_with_tag("item 1").perform_click();
    ui.on_node_with_tag("item 1").perform_click();
    scroll_to(&mut ui, 50);
    ui.on_node_with_text("item 1: 2").assert_does_not_exist();
    ui.on_node_with_tag("item 51").perform_click();
    scroll_to(&mut ui, 0);
    ui.on_node_with_text("item 1: 2").assert_exists();
    ui.on_node_with_text("item 0: 0").assert_exists();
    // Across the host's recreation, those scrolled away too.
    let mut ui = recreate(ui, CountingList);
    ui.on_node_with_text("item 1: 2").assert_exists();
    scroll_to(&mut ui, 50);
    ui.on_node_with_text("item 51: 1").assert_exists();
}

#[test]
fn lazy_grid_cells_keep_their_saveable_state_when_scrolled_away() {
    thread_local! { static GRID: Cell<Option<LazyGridState>> = const { Cell::new(None) }; }
    // The grid's rows are a lazy list's items: each row's provider keeps its cells' state.
    let content = || {
        let state = remember_lazy_grid_state!();
        GRID.set(Some(state));
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 200.dp())),
            state = state,
            |scope: LazyGridScope| {
                scope.items(300, |index| {
                    let clicks = remember_saveable(|| mutable_state_of(0u32));
                    Box!(
                        modifier = Modifier
                            .height(50.dp())
                            .clickable(move || clicks.update(|count| count + 1))
                            .test_tag(format!("cell {index}")),
                        {
                            BasicText!("cell {index}: {}", clicks.get());
                        }
                    );
                });
            }
        );
    };
    let mut ui = compose_test(content);
    ui.on_node_with_tag("cell 4").perform_click();
    ui.on_node_with_tag("cell 5").perform_click();
    ui.on_node_with_tag("cell 5").perform_click();
    GRID.get().unwrap().scroll_to_item(150, 0);
    ui.wait_for_idle();
    ui.on_node_with_tag("cell 4").assert_does_not_exist();
    GRID.get().unwrap().scroll_to_item(0, 0);
    ui.wait_for_idle();
    ui.on_node_with_text("cell 3: 0").assert_exists();
    ui.on_node_with_text("cell 4: 1").assert_exists();
    ui.on_node_with_text("cell 5: 2").assert_exists();
}
