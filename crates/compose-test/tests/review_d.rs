//! Phase D review: failing tests for the test rule.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

#[path = "../../compose-ui/tests/support/mod.rs"]
mod support;

use compose_test::*;
use compose_ui::*;
use support::*;

/// `onAllNodes(matcher)[index].assertDoesNotExist()` passes in Compose when fewer than
/// `index + 1` nodes match. compose-test's `assert_does_not_exist` ignores the index and fails
/// as soon as any node matches.
#[test]
fn an_indexed_node_past_the_matches_does_not_exist() {
    let mut ui = compose_test(|| {
        Text!("A");
    });
    ui.on_all_nodes(has_text("A")).get(0).assert_exists();
    ui.on_all_nodes(has_text("A")).get(1).assert_does_not_exist();
}

/// An editable node: a focus target (when enabled) taking text input, logging what it gets.
struct Field {
    name: &'static str,
    enabled: bool,
}

struct FieldNode {
    name: &'static str,
    enabled: bool,
}

impl ModifierNodeElement for Field {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(FieldNode { name: self.name, enabled: self.enabled })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<FieldNode>() {
            node.name = self.name;
            node.enabled = self.enabled;
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other
            .downcast_ref::<Field>()
            .is_some_and(|other| other.name == self.name && other.enabled == self.enabled)
    }
}

impl ModifierNode for FieldNode {
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl FocusTargetModifierNode for FieldNode {
    fn can_focus(&self) -> bool {
        self.enabled
    }
}

impl KeyInputModifierNode for FieldNode {
    fn on_text_input(&mut self, text: &str, _: &mut EventContext<'_>) -> bool {
        log(format!("{} got {text}", self.name));
        true
    }
}

impl SemanticsModifierNode for FieldNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.editable = true;
        config.editable_text = Some(self.name.into());
        config.disabled = !self.enabled;
    }
}

/// `perform_text_input` focuses the node and sends the text to whatever has focus. When the
/// node can't take focus (a disabled field), `request_focus` silently does nothing and the text
/// goes to the field focused before: typing into B edits A. Compose's `performTextInput` acts
/// on the node itself, and fails on a node that can't take text.
#[test]
fn text_input_into_an_unfocusable_field_does_not_reach_another_field() {
    let mut ui = compose_test(|| {
        Column!({
            Leaf(Modifier.size(10.dp()).then_custom(Field { name: "a", enabled: true }));
            Leaf(Modifier.size(10.dp()).then_custom(Field { name: "b", enabled: false }));
        });
    });
    take_log();
    ui.on_node(has_text("a")).perform_text_input("first");
    let typed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ui.on_node(has_text("b")).perform_text_input("second");
    }));
    let log = take_log();
    assert!(
        !log.contains(&"a got second".to_string()),
        "the text for b went to a: {log:?} (perform_text_input panicked: {})",
        typed.is_err()
    );
}
