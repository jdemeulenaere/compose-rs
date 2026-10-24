use compose::test::*;
use compose::ui::{IntSize, Role};
use todo::{TodoApp, TodoAppWith};

fn add(ui: &mut ComposeTestRule, title: &str) {
    ui.on_node_with_tag("draft").perform_text_input(title);
    ui.on_node_with_text("Add").perform_click();
}

#[test]
fn adding_toggling_filtering_and_removing() {
    let mut ui = compose_test(TodoApp);
    for title in ["milk", "eggs", "bread"] {
        add(&mut ui, title);
    }
    ui.on_node_with_tag("draft").assert_text_equals(&[""]);
    assert_eq!(ui.count_nodes(&has_role(Role::Checkbox)), 3);
    ui.on_all_nodes(has_role(Role::Checkbox), 1).perform_click();
    ui.on_all_nodes(has_role(Role::Checkbox), 1).assert_is_on();
    ui.on_node_with_text("Done").perform_click();
    assert_eq!(ui.count_nodes(&has_role(Role::Checkbox)), 1);
    ui.on_node_with_text("eggs").assert_exists();
    ui.on_node_with_text("Active").perform_click();
    assert_eq!(ui.count_nodes(&has_role(Role::Checkbox)), 2);
    ui.on_node_with_text("All").perform_click();
    ui.on_all_nodes(has_text("Remove"), 0).perform_click();
    ui.on_node_with_text("milk").assert_does_not_exist();
    assert_eq!(ui.count_nodes(&has_role(Role::Checkbox)), 2);
}

#[test]
fn toggling_one_item_recomposes_one_row() {
    let mut ui = compose_test(TodoApp);
    for title in ["milk", "eggs", "bread"] {
        add(&mut ui, title);
    }
    let before = ui.recomposition_counts();
    ui.on_all_nodes(has_role(Role::Checkbox), 1).perform_click();
    let recomposed = ui.recomposition_counts().since(&before);
    // The row's content reads `done`, so the layout invoking it re-runs it: one checkbox, and
    // nothing above the row (not even `TodoRow`'s own body).
    assert_one_row(&recomposed);
}

fn assert_one_row(recomposed: &compose::runtime::platform::RecompositionCounts) {
    let names = recomposed.names();
    assert_eq!(recomposed.scopes_named("Checkbox"), 1, "{names:?}");
    assert_eq!(recomposed.scopes_named("TodoAppWith"), 0, "{names:?}");
    assert!(recomposed.scopes_named("TodoRow") <= 1, "{names:?}");
    // The button the pointer left composes again for each frame of its elevation's animation
    // (150 ms back from its hover elevation), its surface's box and layouts with it, as
    // Compose's `Button` reads its animated elevation in composition (D526); nothing else does.
    let animating = ["Button", "Box", "Layout"];
    let total: u64 = names
        .iter()
        .filter(|(name, _)| !animating.contains(name))
        .map(|(_, count)| count)
        .sum();
    assert!(total <= 3, "{names:?}");
    assert!(recomposed.scopes_named("Button") <= 12, "{names:?}");
}

#[test]
fn a_thousand_items_toggle_one_row() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 100_000), || TodoAppWith(1000));
    assert_eq!(ui.count_nodes(&has_role(Role::Checkbox)), 1000);
    let before = ui.recomposition_counts();
    ui.on_all_nodes(has_role(Role::Checkbox), 500).perform_click();
    let recomposed = ui.recomposition_counts().since(&before);
    assert_one_row(&recomposed);
    ui.on_all_nodes(has_role(Role::Checkbox), 500).assert_is_on();
}

#[test]
fn the_session_matches_its_transcript() {
    let mut ui = compose_test(TodoApp);
    check_script(&mut ui, concat!(env!("CARGO_MANIFEST_DIR"), "/tests/todo.script"));
}
