//! Matchers over a node in its tree and the selectors walking it, as Compose's `Filters.kt`
//! (`hasParent`, `hasAnyAncestor`…) and `Selectors.kt` (`onChildren`, `onParent`…).

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Modifier, UnitExt};

/// A node tagged `tag` around `content`, as Compose's test `BoundaryNode`.
#[composable]
fn Node(tag: &'static str, content: impl Fn() + 'static) {
    Box!(modifier = Modifier.size(10.dp()).test_tag(tag), {
        content();
    });
}

/// A node tagged `tag` without content.
#[composable]
fn Leaf(tag: &'static str) {
    Box!(modifier = Modifier.size(10.dp()).test_tag(tag));
}

#[track_caller]
fn expect_error(action: impl FnOnce()) -> String {
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(action));
    let panic = failed.expect_err("an assertion error");
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default()
}

fn tag(tag: &str) -> Matcher {
    has_test_tag(tag)
}

// ---- Tree matchers ---------------------------------------------------------------------------

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/predicates/HasParentTest.kt, findByParent_oneSubtree_oneChild_matches, findByParent_oneSubtree_twoChildren_matches, findByParent_twoSubtrees_twoChildren_matches
#[test]
fn has_parent_matches_the_children_of_a_matching_node() {
    let mut ui = compose_test(|| {
        Leaf("Node");
        Node!("Parent", {
            Leaf("Child1");
            Leaf("Child2");
        });
        Node!("Parent", {
            Leaf("Child1");
            Leaf("Child2");
        });
    });
    ui.on_all_nodes(has_parent(tag("Parent"))).assert_count_equals(4);
    ui.on_all_nodes(has_parent(tag("Parent")) & tag("Child1")).assert_count_equals(2);
    ui.on_node(has_parent(tag("Node"))).assert_does_not_exist();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/predicates/HasAnyAncestorTest.kt, findByAncestor_oneAncestor_oneMatch, findByAncestor_oneAncestor_twoChildren_twoMatches, findByAncestor_twoAncestors_oneMatch, findByAncestor_twoAncestors_twoMatches, findByAncestor_justSelf_noMatch, findByAncestor_oneAncestor_noMatch
#[test]
fn has_any_ancestor_looks_up_to_the_root_but_not_at_the_node() {
    let mut ui = compose_test(|| {
        Leaf("Node");
        Node!("Grandparent", {
            Node!("Parent", {
                Leaf("Child1");
                Leaf("Child2");
            });
        });
    });
    ui.on_all_nodes(has_any_ancestor(tag("Parent"))).assert_count_equals(2);
    ui.on_all_nodes(has_any_ancestor(tag("Grandparent")) & !tag("Parent")).assert_count_equals(2);
    ui.on_all_nodes(has_any_ancestor(tag("Parent").or(tag("Grandparent")))).assert_count_equals(3);
    ui.on_node(has_any_ancestor(tag("Node"))).assert_does_not_exist();
    ui.on_node(has_any_ancestor(tag("Child1"))).assert_does_not_exist();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/predicates/HasAnyChildTest.kt, findByChild_oneSubtree_oneChild_matches, findByChild_threeSubtrees_twoChildren_matches
#[test]
fn has_any_child_matches_the_parents_of_a_matching_node() {
    let mut ui = compose_test(|| {
        Leaf("Node");
        Node!("Parent1", {
            Leaf("Child1");
            Leaf("Child2");
        });
        Node!("Parent2", {
            Leaf("Child2");
            Leaf("Child1");
        });
        Node!("Parent3", {
            Leaf("Child2");
            Leaf("Child3");
        });
    });
    ui.on_all_nodes(has_any_child(tag("Child1"))).assert_count_equals(2);
    ui.on_node(has_any_child(tag("Child3"))).assert(tag("Parent3"));
    ui.on_node(has_any_child(tag("Parent1")) & !is_root()).assert_does_not_exist();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/predicates/HasAnyDescendantTest.kt, findByDescendant_directDescendant_matches, findByDescendant_indirectDescendant_matches
#[test]
fn has_any_descendant_looks_down_the_whole_subtree() {
    let mut ui = compose_test(|| {
        Leaf("Node");
        Node!("Grandparent", {
            Node!("Parent", {
                Leaf("Child");
            });
        });
    });
    ui.on_node(has_any_descendant(tag("Child")) & tag("Parent")).assert(tag("Parent"));
    ui.on_node(has_any_descendant(tag("Child")) & !tag("Parent") & !is_root())
        .assert(tag("Grandparent"));
    ui.on_node(has_any_descendant(tag("Node")) & !is_root()).assert_does_not_exist();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/predicates/HasAnySiblingTest.kt, findBySibling_oneSubtree_oneSibling_matches, findBySibling_oneSubtree_twoSiblings_matchesTwo, findBySibling_oneSubtree_oneSibling_matchesTwo, findBySibling_twoSubtrees_twoChildren_matches, findBySibling_noSiblings_nothingFound
#[test]
fn has_any_sibling_matches_the_other_children_of_the_parent() {
    let mut ui = compose_test(|| {
        Node!("Parent", {
            Leaf("Me");
            Leaf("Sibling");
            Leaf("SomeoneElse");
        });
        Node!("Twins", {
            Leaf("Twin");
            Leaf("Twin");
        });
        Node!("Lonely", {
            Leaf("Alone");
        });
    });
    ui.on_all_nodes(has_any_sibling(tag("Sibling"))).assert_count_equals(2);
    ui.on_all_nodes(has_any_sibling(tag("Twin"))).assert_count_equals(2);
    ui.on_node(has_any_sibling(tag("Alone"))).assert_does_not_exist();
}

/// `is_root` matches the root alone.
#[test]
fn is_root_matches_the_root() {
    let mut ui = compose_test(|| Leaf("Node"));
    ui.on_all_nodes(is_root()).assert_count_equals(1);
    ui.on_root().on_children().assert_count_equals(1);
    ui.on_root().on_child().assert(tag("Node"));
}

// ---- Selectors -------------------------------------------------------------------------------

#[composable]
fn Family() {
    Node!("Parent", {
        Leaf("Child1");
        Leaf("Child2");
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/selectors/ChildrenSelectorTest.kt, twoChildren, noChildren
#[test]
fn on_children_selects_the_children_in_order() {
    let mut ui = compose_test(Family);
    ui.on_node_with_tag("Parent").on_children().assert_count_equals(2);
    ui.on_node_with_tag("Parent").on_children().get(0).assert(tag("Child1"));
    ui.on_node_with_tag("Parent").on_children().get(1).assert(tag("Child2"));
    ui.on_node_with_tag("Child1").on_children().assert_count_equals(0);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/selectors/ChildSelectorTest.kt
#[test]
fn on_child_selects_the_only_child_or_fails() {
    let mut ui = compose_test(|| {
        Node!("Single", {
            Leaf("Child");
        });
        Family();
    });
    ui.on_node_with_tag("Single").on_child().assert(tag("Child"));
    expect_error(|| {
        ui.on_node(tag("Parent") & has_any_child(tag("Child1"))).on_child().assert_exists();
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/selectors/ParentSelectorTest.kt, oneParent, noParent, noParent_fail
#[test]
fn on_parent_selects_the_parent_and_nothing_past_the_root() {
    let mut ui = compose_test(Family);
    ui.on_node_with_tag("Child1").on_parent().assert(tag("Parent"));
    ui.on_node_with_tag("Parent").on_parent().on_parent().assert_does_not_exist();
    let message = expect_error(|| {
        ui.on_node_with_tag("Parent").on_parent().on_parent().assert_exists();
    });
    assert!(message.contains("((with tag \"Parent\").parent).parent"), "{message}");
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/selectors/SiblingsSelectorTest.kt, SiblingSelectorTest.kt
#[test]
fn on_siblings_selects_the_other_children() {
    let mut ui = compose_test(|| {
        Node!("Parent", {
            Leaf("Child1");
            Leaf("Child2");
            Leaf("Child3");
        });
    });
    ui.on_node_with_tag("Child2").on_siblings().assert_count_equals(2);
    ui.on_node_with_tag("Child2").on_siblings().get(1).assert(tag("Child3"));
    let mut two = compose_test(Family);
    two.on_node_with_tag("Child1").on_sibling().assert(tag("Child2"));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/selectors/AncestorsSelectorTest.kt, DescendantsSelectorTest.kt
#[test]
fn ancestors_and_descendants_are_selected_nearest_first_and_in_order() {
    let mut ui = compose_test(|| {
        Node!("Grandparent", {
            Node!("Parent", {
                Leaf("Child");
            });
        });
    });
    ui.on_node_with_tag("Child").on_ancestors().assert_count_equals(3);
    ui.on_node_with_tag("Child").on_ancestors().on_first().assert(tag("Parent"));
    ui.on_node_with_tag("Child").on_ancestors().on_last().assert(is_root());
    ui.on_node_with_tag("Grandparent").on_descendants().assert_count_equals(2);
    ui.on_node_with_tag("Grandparent").on_descendants().on_last().assert(tag("Child"));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/selectors/AddIndexSelectorTest.kt, getFirst, getAtIndex, getAtIndex_wrongIndex_fail, getAtIndex_noItems, getAtIndex_noItems_fail
#[test]
fn an_index_selects_one_node_or_says_how_many_there_are() {
    let mut ui = compose_test(Family);
    ui.on_node_with_tag("Parent").on_children().on_first().assert(tag("Child1"));
    ui.on_node_with_tag("Parent").on_child_at(1).assert(tag("Child2"));
    let message = expect_error(|| {
        ui.on_node_with_tag("Parent").on_child_at(2).assert_exists();
    });
    assert!(message.contains("can't retrieve node at index '2'"), "{message}");
    assert!(message.contains("there are only 2 nodes"), "{message}");
    ui.on_node_with_tag("Child1").on_child_at(2).assert_does_not_exist();
    let message = expect_error(|| {
        ui.on_node_with_tag("Child1").on_child_at(2).assert_exists();
    });
    assert!(message.contains("there are no existing nodes for that selector"), "{message}");
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/selectors/LastNodeSelectorTest.kt, twoNodes_getLast, zeroNodes_getLast, zeroNodes_getLast_fail
#[test]
fn on_last_selects_the_last_node_if_any() {
    let mut ui = compose_test(Family);
    ui.on_node_with_tag("Parent").on_children().on_last().assert(tag("Child2"));
    ui.on_node_with_tag("Child1").on_children().on_last().assert_does_not_exist();
    let message = expect_error(|| {
        ui.on_node_with_tag("Child1").on_children().on_last().assert_exists();
    });
    assert!(
        message.contains("(((with tag \"Child1\").children).last)") || message.contains("found 0"),
        "{message}"
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/selectors/FilterSelectorTest.kt, FilterToOneSelectorTest.kt
#[test]
fn filters_keep_the_matching_nodes() {
    let mut ui = compose_test(Family);
    ui.on_node_with_tag("Parent").on_children().filter(tag("Child1")).assert_count_equals(1);
    ui.on_node_with_tag("Parent").on_children().filter_to_one(tag("Child1")).assert(tag("Child1"));
    let message = expect_error(|| {
        ui.on_node_with_tag("Parent")
            .on_children()
            .filter_to_one(tag("Child1").or(tag("Child2")))
            .assert_exists();
    });
    assert!(message.contains("found 2"), "{message}");
    ui.on_node_with_tag("Parent").on_children().filter_to_one(tag("Child")).assert_does_not_exist();
}

// ---- Collections -----------------------------------------------------------------------------

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/FindAllTest.kt
#[test]
fn all_matching_nodes_are_found_in_tree_order() {
    let mut ui = compose_test(|| {
        Leaf("a");
        Node!("a", {
            Leaf("a");
        });
    });
    let found = ui.on_all_nodes_with_tag("a").fetch_semantics_nodes();
    assert_eq!(found.len(), 3);
    assert_eq!(found[1].children.len(), 1, "the second is the one around the third");
    ui.on_all_nodes_with_tag("b").assert_count_equals(0);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/assertions/AssertAllTest.kt, AssertAnyTest.kt
#[test]
fn all_or_any_of_the_nodes_satisfy_a_matcher() {
    let mut ui = compose_test(Family);
    ui.on_node_with_tag("Parent").on_children().assert_all(has_parent(tag("Parent")));
    ui.on_node_with_tag("Parent").on_children().assert_any(tag("Child2"));
    // Over no node, every node satisfies anything and none does.
    ui.on_node_with_tag("Child1").on_children().assert_all(tag("anything"));
    let message = expect_error(|| {
        ui.on_node_with_tag("Parent").on_children().assert_all(tag("Child1"));
    });
    assert!(message.contains("assertAll"), "{message}");
    expect_error(|| {
        ui.on_node_with_tag("Parent").on_children().assert_any(tag("Child3"));
    });
    expect_error(|| {
        ui.on_node_with_tag("Child1").on_children().assert_any(tag("anything"));
    });
}

/// Finders look in the merged tree unless told otherwise: a merging parent hides its
/// children's nodes there.
#[test]
fn the_unmerged_tree_shows_what_merging_hides() {
    let mut ui = compose_test(|| {
        Box!(
            modifier = Modifier
                .semantics(|config| config.is_merging_semantics_of_descendants = true)
                .test_tag("parent"),
            {
                Leaf("child");
            }
        );
    });
    ui.on_node_with_tag("child").assert_does_not_exist();
    ui.on_node_with_tag_unmerged("child").assert_exists();
    ui.on_node_with_tag_unmerged("child").on_parent().assert(tag("parent"));
    ui.on_all_nodes_unmerged(tag("child")).assert_count_equals(1);
}
