use compose::material3::MaterialTheme;
use compose::test::*;
use counter::Counter;

#[test]
fn counting_to_three() {
    let mut ui = compose_test(Counter);
    for _ in 0..3 {
        ui.on_node(has_text_containing("Count")).perform_click();
    }
    ui.on_node_with_text("Count: 3").assert_exists();
    ui.on_node_with_text("Reset").assert_is_enabled();
    let _ = MaterialTheme::color_scheme;
}

#[test]
fn the_session_matches_its_transcript() {
    let mut ui = compose_test(Counter);
    check_script(&mut ui, concat!(env!("CARGO_MANIFEST_DIR"), "/tests/counter.script"));
}

#[test]
fn a_click_reruns_only_what_reads_the_count() {
    let mut ui = compose_test(Counter);
    let before = ui.recomposition_counts();
    ui.on_node_with_text("Count: 0").perform_click();
    let ran = ui.recomposition_counts().since(&before);
    assert_eq!(ran.scopes_named("Counter"), 0, "{:?}", ran.names());
}
