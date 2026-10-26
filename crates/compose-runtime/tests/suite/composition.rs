//! Compose's `CompositionTests`, adapted: the core of composition (inserts, moves, replaces,
//! skipping, `remember`, remember observers, invalidation and observation scopes), on the mock
//! views of [`crate::mock`].

#![allow(non_snake_case)]

use compose_runtime::*;

use crate::mock::*;

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, simple
#[test]
fn a_text_is_composed() {
    let mut test = composition_test();
    test.compose(|| Text("Hello!"));
    test.validate(|v| v.text("Hello!"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, simpleChanges
#[test]
fn a_changed_state_updates_its_text() {
    let mut test = composition_test();
    let name = mutable_state_of(String::from("Bob"));
    let handle = name.handle();
    test.compose(move || Text(&format!("Hello {}", handle.get())));
    test.validate(move |v| v.text(&format!("Hello {}", handle.get())));
    name.set("Robert".into());
    test.expect_changes();
    test.revalidate();
    test.validate(|v| v.text("Hello Robert"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testComposeAModel
#[test]
fn a_model_is_composed() {
    let mut test = composition_test();
    let Contacts { bob, jon, steve } = contacts();
    let model = ContactModel::new(vec![bob.clone(), jon.clone(), steve.clone()]);
    let composed = model.clone();
    test.compose(move || SelectContact(composed.clone()));
    test.validate(move |v| {
        v.linear(|v| {
            v.linear(|v| {
                v.text("Filter:");
                v.edit("");
            });
            v.linear(|v| {
                v.text("Contacts:");
                v.linear(|v| {
                    v.contact(&bob);
                    v.contact(&jon);
                    v.contact(&steve);
                });
            });
        });
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRecomposeWithoutChanges
#[test]
fn recomposing_nothing_changes_nothing() {
    let mut test = composition_test();
    let Contacts { bob, jon, steve } = contacts();
    let model = ContactModel::new(vec![bob, jon, steve]);
    let composed = model.clone();
    test.compose(move || SelectContact(composed.clone()));
    test.expect_no_changes();
    test.validate(move |v| v.select_contact(&model));
}

/// The contacts list of `SelectContact`'s views, the rest skipped.
fn expect_contacts(v: &mut Expected, contacts: &[&ContactRef]) {
    v.linear(|v| {
        v.skip(1);
        v.linear(|v| {
            v.skip(1);
            v.linear(|v| {
                for contact in contacts {
                    v.contact(contact);
                }
            });
        });
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testInsertAContact
#[test]
fn a_contact_is_inserted() {
    let mut test = composition_test();
    let Contacts { bob, jon, steve } = contacts();
    let model = ContactModel::new(vec![bob.clone(), jon.clone()]);
    let scope = ScopeHolder::new();
    let (composed, held) = (model.clone(), scope.clone());
    test.compose(move || {
        held.capture();
        SelectContact(composed.clone());
    });
    let (b, j) = (bob.clone(), jon.clone());
    test.validate(move |v| expect_contacts(v, &[&b, &j]));
    model.add(steve.clone(), Some(&bob));
    scope.invalidate();
    test.expect_changes();
    test.validate(move |v| expect_contacts(v, &[&bob, &steve, &jon]));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testMoveAContact
#[test]
fn a_contact_is_moved() {
    let mut test = composition_test();
    let Contacts { bob, jon, steve } = contacts();
    let model = ContactModel::new(vec![bob.clone(), steve.clone(), jon.clone()]);
    let scope = ScopeHolder::new();
    let (composed, held) = (model.clone(), scope.clone());
    test.compose(move || {
        held.capture();
        SelectContact(composed.clone());
    });
    model.move_after(&steve, Some(&jon));
    scope.invalidate();
    test.expect_changes();
    test.validate(move |v| expect_contacts(v, &[&bob, &jon, &steve]));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testChangeTheFilter
#[test]
fn the_filter_removes_contacts() {
    let mut test = composition_test();
    let Contacts { bob, jon, steve } = contacts();
    let model = ContactModel::new(vec![bob, steve, jon.clone()]);
    let scope = ScopeHolder::new();
    let (composed, held) = (model.clone(), scope.clone());
    test.compose(move || {
        held.capture();
        SelectContact(composed.clone());
    });
    model.set_filter("Jon");
    scope.invalidate();
    test.expect_changes();
    test.validate(move |v| expect_contacts(v, &[&jon]));
}

/// A node counting the changes to its property, as `SetUpdateNode` in `composeNodeSetVsUpdate`.
#[derive(Debug)]
struct SetUpdateNode {
    property: String,
    change_count: u32,
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, composeNodeSetVsUpdate
#[test]
fn an_updater_sets_on_new_nodes_and_updates_only_old_ones() {
    use compose_runtime::platform::NodeTree;
    type Nodes = NodeTree<SetUpdateNode>;
    let composition = Composition::new(Nodes::new());
    let value = mutable_state_of(String::from("initial"));
    let handle = value.handle();
    let make = move || SetUpdateNode { property: handle.get(), change_count: 0 };
    let set = |node: &mut SetUpdateNode, value: &String| {
        node.property = value.clone();
        node.change_count += 1;
    };
    composition.set_content(move || {
        ComposeNode::<Nodes>(make, |updater| updater.set(handle.get(), set));
        ComposeNode::<Nodes>(make, |updater| updater.update(handle.get(), set));
    });
    let nodes = |composition: &Composition| {
        composition.with_applier(|nodes: &mut Nodes| {
            let root = nodes.root();
            let children: Vec<_> = nodes.children(root).collect();
            children
                .iter()
                .map(|&node| {
                    let node = nodes.get(node).expect("a live node");
                    (node.property.clone(), node.change_count)
                })
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        nodes(&composition),
        [("initial".to_string(), 1), ("initial".to_string(), 0)],
        "set runs on a new node, update doesn't"
    );
    value.set("changed".into());
    composition.recompose();
    assert_eq!(
        nodes(&composition),
        [("changed".to_string(), 2), ("changed".to_string(), 1)],
        "both run on a changed value"
    );
}
