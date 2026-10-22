// ANCHOR: test
use compose::test::*;
use guide::getting_started::Hello;

#[test]
fn greets_by_name() {
    let mut ui = compose_test(Hello);
    ui.on_node_with_text("Hello, world!").assert_exists();
    ui.on_node_with_tag("name").perform_text_input("Ada");
    ui.on_node_with_text("Hello, Ada!").assert_exists();
}
// ANCHOR_END: test
