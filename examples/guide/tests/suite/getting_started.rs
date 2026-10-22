//! The first test (docs/guide/README.md). In the guide's layout this is a test of
//! `examples/hello`, whose library is `hello`; here `hello` names the module that plays its part.

use guide::getting_started as hello;

// ANCHOR: test
// examples/hello/tests/hello.rs
use compose::test::*;
use hello::Hello;

#[test]
fn greets_by_name() {
    let mut ui = compose_test(Hello);
    ui.on_node_with_text("Hello, world!").assert_exists();
    ui.on_node_with_tag("name").perform_text_input("Ada");
    ui.on_node_with_text("Hello, Ada!").assert_exists();
}
// ANCHOR_END: test
