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

#[composable]
fn SkippableHidingText(label: String, visible: State<bool>) {
    if visible.get() {
        Text(&label);
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testImminentInvalidationForCurrentGroup
#[test]
fn a_write_to_what_a_later_composable_reads_recomposes_it_in_the_same_pass() {
    #[composable]
    fn Content(new_show_child: MutableState<bool>, compositions: Counter) {
        let show_child = remember(|| mutable_state_of(new_show_child.get()));
        show_child.set(new_show_child.get());
        SkippableHidingText("Child".into(), show_child.as_state());
        Text(&format!("compositions = {}", compositions.increment()));
    }
    let mut test = composition_test();
    let new_show_child = mutable_state_of(true);
    let compositions = Counter::new();
    let (s, c) = (new_show_child.handle(), compositions.clone());
    test.compose(move || Content(s, c.clone()));
    test.validate(|v| {
        v.text("Child");
        v.text("compositions = 1");
    });
    new_show_child.set(false);
    assert_eq!(test.advance_count(), 1, "content should settle in one composition");
    test.validate(|v| v.text("compositions = 2"));
    assert!(!test.composition().has_invalidations(), "no invalidation left behind");
}

thread_local! {
    static OBSERVATION_SCOPE_CALLS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static OBSERVATION_SCOPE_FORWARD_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[composable]
fn calculate_value(state: MutableState<String>) -> String {
    remember(move || state.get())
}

#[composable]
fn ObservationScopesTest(state: MutableState<String>, forward_write: bool) {
    OBSERVATION_SCOPE_CALLS.with(|calls| calls.set(calls.get() + 1));
    calculate_value(state);
    OBSERVATION_SCOPE_FORWARD_WRITE.with(|value| value.set(forward_write));
}

#[composable]
fn ForwardWrite(state: MutableState<String>) {
    state.update(|value| format!("{value}, forward write"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testObservationScopes_ReadInRemember
#[test]
fn a_backward_write_recomposes_a_scope_that_read_in_remember() {
    #[composable]
    fn Content(
        state: MutableState<String>,
        main_state: MutableState<String>,
        do_forward_write: MutableState<bool>,
    ) {
        Text(&main_state.get());
        ObservationScopesTest(state, do_forward_write.get());
        if do_forward_write.get() {
            ForwardWrite(state);
        }
    }
    let mut test = composition_test();
    let state = mutable_state_of(String::from("state"));
    let main_state = mutable_state_of(String::from("main state"));
    let do_forward_write = mutable_state_of(false);
    let (s, m, d) = (state.handle(), main_state.handle(), do_forward_write.handle());
    test.compose(move || Content(s, m, d));
    // Set up the case by skipping ObservationScopesTest.
    main_state.update(|value| format!("{value}, changed"));
    test.advance();
    // The forward write after skipping ObservationScopesTest: a backward write through the
    // read in its remember, transitory as the next writes are forward writes.
    do_forward_write.set(true);
    test.advance_ignoring_pending_work();
    assert!(
        OBSERVATION_SCOPE_FORWARD_WRITE.with(std::cell::Cell::get),
        "ObservationScopesTest saw the new argument"
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testStateWriteInApplier
#[test]
fn a_state_written_by_the_applier_recomposes_its_reader_in_the_next_pass() {
    use compose_runtime::platform::{Applier, NodeRef, NodeTree};
    /// Counts removals in a state, as `MutateOnRemoveApplier`.
    struct MutateOnRemove {
        tree: NodeTree<()>,
        removed: MutableState<usize>,
        inserted: usize,
    }
    impl Applier for MutateOnRemove {
        type Node = ();
        fn root(&self) -> NodeRef {
            self.tree.root()
        }
        fn insert(&mut self, parent: NodeRef, index: usize, node: ()) -> NodeRef {
            self.inserted += 1;
            Applier::insert(&mut self.tree, parent, index, node)
        }
        fn remove(&mut self, parent: NodeRef, index: usize, count: usize) {
            self.removed.update(|removed| removed + count);
            Applier::remove(&mut self.tree, parent, index, count)
        }
        fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize) {
            Applier::move_nodes(&mut self.tree, parent, from, to, count)
        }
        fn release(&mut self, node: NodeRef) {
            Applier::release(&mut self.tree, node)
        }
    }
    #[composable]
    fn Content(
        removed: MutableState<usize>,
        should_emit_node: MutableState<bool>,
        compositions: Counter,
    ) {
        compositions.increment();
        // Read here, so that the node's removal invalidates this.
        removed.get();
        if should_emit_node.get() {
            ComposeNode::<MutateOnRemove>(|| (), |_| {});
        }
    }
    let removed = mutable_state_of(0);
    let should_emit_node = mutable_state_of(true);
    let compositions = Counter::new();
    let composition = Composition::new(MutateOnRemove {
        tree: NodeTree::new(),
        removed: removed.handle(),
        inserted: 0,
    });
    let (r, s, c) = (removed.handle(), should_emit_node.handle(), compositions.clone());
    composition.set_content(move || Content(r, s, c.clone()));
    assert_eq!(composition.with_applier(|applier: &mut MutateOnRemove| applier.inserted), 1);
    should_emit_node.set(false);
    assert!(composition.recompose());
    assert_eq!(removed.get(), 1, "the removal was applied");
    assert_eq!(compositions.get(), 2, "the applier's write waits for the next pass");
    assert!(composition.recompose());
    assert_eq!(compositions.get(), 3);
    assert!(!composition.recompose());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testInvalidationOfRemovedContent
#[test]
fn a_backward_invalidation_of_removed_content_is_ignored() {
    thread_local! {
        static S1_SCOPE: ScopeHolder = ScopeHolder::new();
        static PERFORM_BACKWARDS_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    }
    #[composable]
    fn S1() {
        S1_SCOPE.with(ScopeHolder::capture);
        Text("In s1");
    }
    #[composable]
    fn S2() {
        Text("In s2");
    }
    #[composable]
    fn Test(view_s1: MutableState<bool>) {
        Text(&view_s1.get().to_string());
        Wrap!({
            if view_s1.get() {
                S1();
            }
            S2();
        });
        if PERFORM_BACKWARDS_WRITE.with(|perform| perform.replace(false)) {
            // The equivalent of a backwards write.
            S1_SCOPE.with(ScopeHolder::invalidate);
        }
    }
    let mut test = composition_test();
    let view_s1 = mutable_state_of(true);
    let v = view_s1.handle();
    test.compose(move || Test(v));
    test.validate(move |expected| {
        expected.text(&v.get().to_string());
        if v.get() {
            expected.text("In s1");
        }
        expected.text("In s2");
    });
    S1_SCOPE.with(ScopeHolder::invalidate);
    PERFORM_BACKWARDS_WRITE.with(|perform| perform.set(true));
    test.expect_no_changes();
    test.revalidate();
    view_s1.set(false);
    PERFORM_BACKWARDS_WRITE.with(|perform| perform.set(true));
    test.expect_changes();
    test.revalidate();
}
