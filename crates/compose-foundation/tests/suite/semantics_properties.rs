//! Semantics as Compose's typed keys (D1250): the merged configuration of a node, each key's
//! merge policy, actions collapsed on one node and merged from descendants, an application's
//! own keys and actions, and the key matchers.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    AccessibilityAction, ActionFn, CustomAccessibilityAction, SemanticsActions,
    SemanticsConfiguration, SemanticsProperties, SemanticsPropertyKey, SemanticsValue,
    SharedString, StringList, UnitExt,
};

fn texts(config: &SemanticsConfiguration) -> Vec<String> {
    config
        .get(&SemanticsProperties::TEXT)
        .into_iter()
        .flatten()
        .map(|text| text.to_string())
        .collect()
}

/// The node tagged `tag`'s configuration, unmerged and merged.
fn configurations(
    ui: &mut ComposeTestRule,
    tag: &str,
) -> (SemanticsConfiguration, SemanticsConfiguration) {
    let unmerged = ui.on_node_with_tag_unmerged(tag).fetch_semantics_node().config;
    let merged = ui.on_node_with_tag(tag).fetch_semantics_node().config;
    (unmerged, merged)
}

fn text(text: &'static str) -> Modifier {
    Modifier.semantics(move |config| config.set_text(text))
}

// ---- The merged configuration -----------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/MergedSemanticsConfigurationTest.kt, singleModifier
#[test]
fn a_single_modifier_merges_to_itself() {
    let mut ui = compose_test(|| {
        Box!(
            modifier = Modifier.semantics(|config| {
                config.set_test_tag("box");
                config.set_text("1");
            })
        );
    });
    let (unmerged, merged) = configurations(&mut ui, "box");
    assert_eq!(texts(&unmerged), ["1"]);
    assert_eq!(merged, unmerged);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/MergedSemanticsConfigurationTest.kt, multipleModifierOnSameNode
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/MergedSemanticsConfigurationTest.kt, multipleModifierOnSameNode_mergingDescendants
/// On one node, the outer modifier's value wins, merging or not.
#[test]
fn the_outer_modifier_wins_on_one_node() {
    for merging in [false, true] {
        let mut ui = compose_test(move || {
            Box!(
                modifier = Modifier
                    .semantics_with(merging, |config| {
                        config.set_test_tag("box");
                        config.set_text("1");
                    })
                    .semantics(|config| config.set_text("2"))
            );
        });
        let (unmerged, merged) = configurations(&mut ui, "box");
        assert_eq!(texts(&unmerged), ["1"]);
        assert_eq!(merged, unmerged);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/MergedSemanticsConfigurationTest.kt, multipleLayoutNodes_default
#[test]
fn a_node_that_does_not_merge_keeps_its_own() {
    let mut ui = compose_test(|| {
        Box!(
            modifier = Modifier.semantics(|config| {
                config.set_test_tag("box");
                config.set_text("1");
            }),
            {
                Box!(modifier = text("2"));
            }
        );
    });
    let (unmerged, merged) = configurations(&mut ui, "box");
    assert_eq!(texts(&unmerged), ["1"]);
    assert_eq!(merged, unmerged);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/MergedSemanticsConfigurationTest.kt, multipleLayoutNodes_isMergingDescendants_oneChild
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/MergedSemanticsConfigurationTest.kt, multipleLayoutNodes_isMergingDescendants_twoChildren
/// A merging node's texts are its own, then its descendants' (in their order: Compose's
/// `SemanticsNode` merges so; its newer `SemanticsInfo.mergedSemanticsConfiguration` walks the
/// children last first).
#[test]
fn a_merging_node_takes_its_childrens_texts() {
    let mut ui = compose_test(|| {
        Row!(
            modifier = Modifier.semantics_with(true, |config| {
                config.set_test_tag("row");
                config.set_text("1");
            }),
            {
                Box!(modifier = text("2.1"));
                Box!(modifier = text("2.2"));
            }
        );
    });
    let (unmerged, merged) = configurations(&mut ui, "row");
    assert_eq!(texts(&unmerged), ["1"]);
    assert_eq!(texts(&merged), ["1", "2.1", "2.2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/MergedSemanticsConfigurationTest.kt, multipleLayoutNodes_isMergingDescendants_clearAndSetSemantics_twoChildren
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/MergedSemanticsConfigurationTest.kt, multipleLayoutNodes_isClearingSemantics_twoChildren
/// A node clearing its descendants' semantics merges none of them.
#[test]
fn a_clearing_node_merges_nothing() {
    for merging in [false, true] {
        let mut ui = compose_test(move || {
            let modifier = if merging {
                Modifier
                    .semantics_with(true, |config| {
                        config.set_test_tag("row");
                        config.set_text("1");
                    })
                    .clear_and_set_semantics(|_| {})
            } else {
                Modifier.clear_and_set_semantics(|config| {
                    config.set_test_tag("row");
                    config.set_text("1");
                })
            };
            Row!(modifier = modifier, {
                Box!(modifier = text("2.1"));
                Box!(modifier = text("2.2"));
            });
        });
        let (unmerged, merged) = configurations(&mut ui, "row");
        assert_eq!(texts(&unmerged), ["1"]);
        assert_eq!(texts(&merged), ["1"]);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/MergedSemanticsConfigurationTest.kt, deepHierarchy
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/MergedSemanticsConfigurationTest.kt, doesNotMergeMergedItems
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/MergedSemanticsConfigurationTest.kt, doesNotIncludeChildrenThatAreCleared
/// Merging goes all the way down, but not into a descendant that merges itself, and only to
/// the clearing node's own properties below one that clears.
#[test]
fn merging_goes_deep_and_stops_at_mergers_and_clearers() {
    #[derive(Clone, Copy, PartialEq)]
    enum Middle {
        Plain,
        Merging,
        Clearing,
    }
    for (middle, expected) in [
        (Middle::Plain, vec!["1", "2.1", "2.2", "3.1", "3.2", "2.3"]),
        (Middle::Merging, vec!["1", "2.1", "2.3"]),
        (Middle::Clearing, vec!["1", "2.1", "2.2", "2.3"]),
    ] {
        let mut ui = compose_test(move || {
            Row!(
                modifier = Modifier.semantics_with(true, |config| {
                    config.set_test_tag("row");
                    config.set_text("1");
                }),
                {
                    Box!(modifier = text("2.1"));
                    let modifier = match middle {
                        Middle::Plain => text("2.2"),
                        Middle::Merging => {
                            Modifier.semantics_with(true, |config| config.set_text("2.2"))
                        }
                        Middle::Clearing => {
                            Modifier.clear_and_set_semantics(|config| config.set_text("2.2"))
                        }
                    };
                    Column!(modifier = modifier, {
                        Box!(modifier = text("3.1"));
                        Box!(modifier = text("3.2"));
                    });
                    Box!(modifier = text("2.3"));
                }
            );
        });
        let (unmerged, merged) = configurations(&mut ui, "row");
        assert_eq!(texts(&unmerged), ["1"]);
        assert_eq!(texts(&merged), expected);
    }
}

// ---- Merge policies ---------------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, hintText_mergePolicy_prefersParentValue
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, hintText_mergePolicy_fallsBackToChildWhenNoParent
#[test]
fn the_default_policy_prefers_the_parents_value() {
    let parent = SharedString::from("Parent hint");
    let child = SharedString::from("Child hint");
    let hint = &SemanticsProperties::HINT_TEXT;
    assert_eq!(hint.merge(Some(&parent), &child), Some(parent));
    assert_eq!(hint.merge(None, &child), Some(child));
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/semantics/SemanticsProperties.kt, ContentDescription, Text, Role, TestTag, TraversalIndex, CustomActions
/// Each key's own policy: texts and descriptions concatenate, the role, the tag and the
/// traversal index never merge up, custom actions concatenate.
#[test]
fn each_key_merges_by_its_policy() {
    let list = |items: &[&str]| -> StringList {
        items.iter().map(|item| SharedString::from(item.to_string())).collect()
    };
    let p = &SemanticsProperties::TEXT;
    assert_eq!(p.merge(Some(&list(&["a"])), &list(&["b", "c"])), Some(list(&["a", "b", "c"])));
    assert_eq!(p.merge(None, &list(&["b"])), Some(list(&["b"])));
    let d = &SemanticsProperties::CONTENT_DESCRIPTION;
    assert_eq!(d.merge(Some(&list(&["a"])), &list(&["b"])), Some(list(&["a", "b"])));
    assert_eq!(
        SemanticsProperties::ROLE.merge(None, &compose_ui::Role::Image),
        None,
        "a role never merges up"
    );
    assert_eq!(SemanticsProperties::TEST_TAG.merge(None, &SharedString::from("t")), None);
    assert_eq!(SemanticsProperties::TRAVERSAL_INDEX.merge(Some(&1.0), &2.0), Some(1.0));
    let first = CustomAccessibilityAction::new("first", || true);
    let second = CustomAccessibilityAction::new("second", || true);
    assert_eq!(
        SemanticsActions::CUSTOM_ACTIONS.merge(Some(&vec![first.clone()]), &vec![second.clone()]),
        Some(vec![first, second])
    );
    assert_eq!(SemanticsProperties::TEXT.name(), "Text");
    assert!(SemanticsProperties::TEXT.is_important_for_accessibility());
    assert!(!SemanticsProperties::TEST_TAG.is_important_for_accessibility());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, paneTitleProperty_unmergedConfig
#[test]
fn a_pane_title_is_its_nodes() {
    let mut ui = compose_test(|| {
        Box!(
            modifier = Modifier.test_tag("pane").semantics(|config| config.set_pane_title("title"))
        );
    });
    let unmerged = ui.on_node_with_tag_unmerged("pane").fetch_semantics_node().config;
    assert_eq!(
        unmerged.get(&SemanticsProperties::PANE_TITLE).map(|title| title.as_str()),
        Some("title")
    );
    ui.on_node_with_tag("pane")
        .assert(expect_value(&SemanticsProperties::PANE_TITLE, SharedString::from("title")));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, paneTitleProperty_mergingThrows
/// A pane title doesn't merge: a merging node holding one fails, with Compose's message.
#[test]
fn merging_a_pane_title_fails() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.clickable(|| {}).test_tag("clickable"), {
            Box!(modifier = Modifier.semantics(|config| config.set_pane_title("title")));
        });
    });
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ui.on_node_with_tag("clickable").fetch_semantics_node();
    }));
    let panic = failed.expect_err("merging a pane title fails");
    let message = panic.downcast_ref::<&str>().map(|text| text.to_string()).unwrap_or_default();
    assert_eq!(
        message,
        "merge function called on unmergeable property PaneTitle. A pane should not be a \
         descendant of a node that merges descendant semantics, such as a clickable."
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, isTraversalGroupProperty_unmergedConfig
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, traversalIndexProperty_unmergedConfig
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, traversalIndexPropertyNull
#[test]
fn traversal_properties_are_set_where_asked() {
    let mut ui = compose_test(|| {
        Column!({
            Box!(
                modifier = Modifier
                    .test_tag("group")
                    .semantics(|config| config.set_is_traversal_group(true)),
                {
                    BasicText!("Hello World", modifier = Modifier.padding(8.dp()));
                }
            );
            Box!(
                modifier =
                    Modifier.semantics(|config| config.set_traversal_index(0.0)).test_tag("index"),
                {
                    BasicText!("Hello World", modifier = Modifier.padding(8.dp()));
                }
            );
            Box!(modifier = Modifier.test_tag("plain"), {
                BasicText!("Hello World", modifier = Modifier.padding(8.dp()));
            });
        });
    });
    ui.on_node_with_tag("group")
        .assert(expect_value(&SemanticsProperties::IS_TRAVERSAL_GROUP, true));
    ui.on_node_with_tag("index").assert(expect_value(&SemanticsProperties::TRAVERSAL_INDEX, 0.0));
    ui.on_node_with_tag("plain").assert(key_not_defined(&SemanticsProperties::TRAVERSAL_INDEX));
}

// ---- Actions ----------------------------------------------------------------------------------

fn label_and_result(
    config: &SemanticsConfiguration,
    key: &'static SemanticsPropertyKey<AccessibilityAction<ActionFn>>,
) -> (Option<String>, Option<bool>) {
    let action = config.get(key);
    (
        action.and_then(|action| action.label.as_deref().map(str::to_string)),
        action.and_then(|action| action.action.as_ref().map(|action| action())),
    )
}

fn label_only(label: &str) -> AccessibilityAction<ActionFn> {
    AccessibilityAction { label: Some(label.to_string().into()), action: None }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, collapseSemanticsActions_prioritizeNonNullAction
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, collapseSemanticsActions_prioritizeNonNullLabel
/// On one node, an action takes the outer modifier's label and function, each where it has
/// one, else the inner's.
#[test]
fn actions_on_one_node_collapse_label_and_function() {
    let mut ui = compose_test(|| {
        Column!({
            Box!(
                modifier =
                    Modifier
                        .test_tag("action")
                        .semantics(
                            |config| config.set(&SemanticsActions::COPY_TEXT, label_only("copy"))
                        )
                        .semantics(|config| config.copy_text(None, || true))
            );
            Box!(
                modifier = Modifier
                    .test_tag("label")
                    .semantics(|config| config.copy_text(None, || false))
                    .semantics(|config| config.copy_text(Some("copy".into()), || true))
            );
        });
    });
    let action = ui.on_node_with_tag("action").fetch_semantics_node().config;
    assert_eq!(
        label_and_result(&action, &SemanticsActions::COPY_TEXT),
        (Some("copy".to_string()), Some(true))
    );
    let label = ui.on_node_with_tag("label").fetch_semantics_node().config;
    assert_eq!(
        label_and_result(&label, &SemanticsActions::COPY_TEXT),
        (Some("copy".to_string()), Some(false))
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, collapseSemanticsActions_changeActionLabel_notMergeDescendants
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, collapseSemanticsActions_changeActionLabel_mergeDescendants
/// An outer modifier labels a clickable's click, which still clicks.
#[test]
fn an_outer_label_names_a_clickables_click() {
    for merging in [false, true] {
        let clicks = Rc::new(Cell::new(0));
        let counted = clicks.clone();
        let mut ui = compose_test(move || {
            let counted = counted.clone();
            Box!(
                modifier = Modifier
                    .test_tag("button")
                    .semantics_with(merging, |config| {
                        config.set(&SemanticsActions::ON_CLICK, label_only("send"))
                    })
                    .clickable(move || counted.set(counted.get() + 1))
                    .size(10.dp())
            );
        });
        let config = ui.on_node_with_tag("button").fetch_semantics_node().config;
        assert_eq!(
            label_and_result(&config, &SemanticsActions::ON_CLICK),
            (Some("send".to_string()), Some(true))
        );
        assert_eq!(clicks.get(), 1);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, mergeSemanticsActions_prioritizeNonNullAction_mergeDescendants_descendantMergeable
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, mergeSemanticsActions_prioritizeNonNullLabel_mergeDescendants_descendantMergeable
/// Merging, an action takes the parent's label and function where it has them, else its
/// descendant's.
#[test]
fn merged_actions_take_label_and_function_apart() {
    let mut ui = compose_test(|| {
        Column!({
            Box!(
                modifier = Modifier.test_tag("action").semantics_with(true, |config| {
                    config.set(&SemanticsActions::EXPAND, label_only("show more"))
                }),
                {
                    Box!(modifier = Modifier.semantics(|config| config.expand(None, || true)));
                }
            );
            Box!(
                modifier = Modifier
                    .test_tag("label")
                    .semantics_with(true, |config| config.expand(None, || false)),
                {
                    Box!(
                        modifier = Modifier.semantics(|config| {
                            config.expand(Some("show more".into()), || true)
                        })
                    );
                }
            );
        });
    });
    let action = ui.on_node_with_tag("action").fetch_semantics_node().config;
    assert_eq!(
        label_and_result(&action, &SemanticsActions::EXPAND),
        (Some("show more".to_string()), Some(true))
    );
    let label = ui.on_node_with_tag("label").fetch_semantics_node().config;
    assert_eq!(
        label_and_result(&label, &SemanticsActions::EXPAND),
        (Some("show more".to_string()), Some(false))
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, mergeSemanticsActions_changeActionLabelNotWork_notMergeDescendants_descendantMergeable
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, mergeSemanticsActions_changeActionLabelNotWork_notMergeDescendants_descendantUnmergeable
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, mergeSemanticsActions_changeActionLabelNotWork_mergeDescendants_descendantUnmergeable
/// A label on a parent doesn't reach a descendant's action when the parent doesn't merge it
/// (it doesn't merge, or the descendant merges itself, a clickable).
#[test]
fn a_parents_label_does_not_reach_an_unmerged_action() {
    for (merging, clickable) in [(false, false), (false, true), (true, true)] {
        let mut ui = compose_test(move || {
            Box!(
                modifier = Modifier.test_tag("parent").semantics_with(merging, |config| {
                    let key = if clickable {
                        &SemanticsActions::ON_CLICK
                    } else {
                        &SemanticsActions::COLLAPSE
                    };
                    config.set(key, label_only("label"))
                }),
                {
                    let child = if clickable {
                        Modifier.clickable(|| {}).size(10.dp())
                    } else {
                        Modifier.semantics(|config| config.collapse(None, || true))
                    };
                    Box!(modifier = child);
                }
            );
        });
        let config = ui.on_node_with_tag("parent").fetch_semantics_node().config;
        let key = if clickable { &SemanticsActions::ON_CLICK } else { &SemanticsActions::COLLAPSE };
        let action = config.get(key).expect("the label");
        assert_eq!(action.label.as_deref(), Some("label"));
        assert!(action.action.is_none(), "{merging} {clickable}");
    }
}

// ---- An application's own keys -----------------------------------------------------------------

/// An application's value type, held through one `Rc`.
#[derive(Clone, PartialEq, Debug)]
struct Rating(u32);

impl SemanticsValue for Rating {}

static RATING: SemanticsPropertyKey<Rating> = SemanticsPropertyKey::new("Rating");

type SetStringFn = dyn Fn(String) -> bool;

/// An action of the application's own signature, as `MyActions.SetString`.
static SET_STRING: SemanticsPropertyKey<AccessibilityAction<SetStringFn>> =
    SemanticsPropertyKey::action("SetString");

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/CallSemanticsActionTest.kt, performSemanticsAction
/// An application declares its own keys and actions as statics; the harness performs an
/// action with its arguments.
#[test]
fn an_applications_own_action_is_performed_with_arguments() {
    let mut ui = compose_test(|| {
        let state = remember(|| mutable_state_of(String::from("Nothing")));
        Column!(
            modifier = Modifier.semantics(move |config| {
                config.set(
                    &SET_STRING,
                    AccessibilityAction::new(
                        None,
                        Rc::new(move |value: String| {
                            state.set(value);
                            true
                        }) as Rc<SetStringFn>,
                    ),
                );
                config.set_content_description(state.get());
                config.set(&RATING, Rating(3));
            }),
            {}
        );
    });
    ui.on_node_with_content_description("Nothing")
        .assert_exists()
        .assert(expect_value(&RATING, Rating(3)))
        .perform_semantics_action_with(&SET_STRING, |set| {
            set("Hello".to_string());
        })
        .assert_does_not_exist();
    ui.on_node_with_content_description("Hello").assert_exists();
}

/// Keys are themselves: two keys of one name and type are two keys.
#[test]
fn keys_are_compared_by_identity() {
    static FIRST: SemanticsPropertyKey<bool> = SemanticsPropertyKey::new("Same");
    static SECOND: SemanticsPropertyKey<bool> = SemanticsPropertyKey::new("Same");
    let mut config = SemanticsConfiguration::default();
    config.set(&FIRST, true);
    assert!(config.contains(&FIRST));
    assert!(!config.contains(&SECOND));
    config.set(&SECOND, false);
    assert_eq!((config.get(&FIRST), config.get(&SECOND)), (Some(&true), Some(&false)));
    assert_eq!(config.remove(&FIRST), Some(true));
    assert!(!config.contains(&FIRST));
}

// ---- Custom actions ---------------------------------------------------------------------------

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/CustomAccessibilityActionsTest.kt, performCustomAccessibilityActionLabelled_failsWhenNoActionMatches
// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/CustomAccessibilityActionsTest.kt, performCustomAccessibilityActionLabelled_failsWhenMultipleActionsMatch
// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/CustomAccessibilityActionsTest.kt, performCustomAccessibilityActionLabelled_invokesActionWhenExactlyOneActionMatches
// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/CustomAccessibilityActionsTest.kt, performCustomAccessibilityActionLabelled_doesntFailWhenActionReturnsFalse
// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/CustomAccessibilityActionsTest.kt, performCustomAccessibilityActionWhere_failsWhenNoActionMatches
#[test]
fn custom_actions_are_performed_by_label() {
    let foo = Rc::new(Cell::new(0));
    let bar = Rc::new(Cell::new(0));
    let (foo_count, bar_count) = (foo.clone(), bar.clone());
    let mut ui = compose_test(move || {
        let (foo_count, bar_count) = (foo_count.clone(), bar_count.clone());
        Column!({
            let (foo_count, bar_count) = (foo_count.clone(), bar_count.clone());
            Box!(
                modifier = Modifier.test_tag("two").semantics(move |config| {
                    let (foo_count, bar_count) = (foo_count.clone(), bar_count.clone());
                    config.set_custom_actions(vec![
                        CustomAccessibilityAction::new("foo", move || {
                            foo_count.set(foo_count.get() + 1);
                            true
                        }),
                        CustomAccessibilityAction::new("bar", move || {
                            bar_count.set(bar_count.get() + 1);
                            true
                        }),
                    ]);
                })
            );
            Box!(
                modifier = Modifier.test_tag("same").semantics(|config| {
                    config.set_custom_actions(vec![
                        CustomAccessibilityAction::new("action", || true),
                        CustomAccessibilityAction::new("action", || true),
                    ]);
                })
            );
            Box!(
                modifier = Modifier.test_tag("false").semantics(|config| {
                    config
                        .set_custom_actions(vec![CustomAccessibilityAction::new("action", || {
                            false
                        })]);
                })
            );
        });
    });
    ui.on_node_with_tag("two").perform_custom_accessibility_action_with_label("foo");
    assert_eq!((foo.get(), bar.get()), (1, 0));
    ui.on_node_with_tag("false").perform_custom_accessibility_action_with_label("action");
    let message = |f: &mut dyn FnMut()| {
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        failed.expect_err("fails").downcast_ref::<String>().cloned().unwrap_or_default()
    };
    let none = message(&mut || {
        ui.on_node_with_tag("two").perform_custom_accessibility_action_with_label("not action");
    });
    assert!(
        none.contains("No custom accessibility actions matched [label is \"not action\"]"),
        "{none}"
    );
    let two = message(&mut || {
        ui.on_node_with_tag("same").perform_custom_accessibility_action_with_label("action");
    });
    assert!(
        two.contains(
            "Expected exactly one custom accessibility action to match [label is \"action\"], \
             but found 2"
        ),
        "{two}"
    );
    let where_none = message(&mut || {
        ui.on_node_with_tag("two")
            .perform_custom_accessibility_action_where("description", |_| false);
    });
    assert!(
        where_none.contains("No custom accessibility actions matched [description]"),
        "{where_none}"
    );
}

// ---- Key matchers -----------------------------------------------------------------------------

// Compose: compose/ui/ui-test/src/commonMain/kotlin/androidx/compose/ui/test/SemanticsMatcher.kt, expectValue, keyIsDefined, keyNotDefined
// Compose: compose/ui/ui-test/src/commonMain/kotlin/androidx/compose/ui/test/Filters.kt, isHeading, hasImeAction, isHiddenFromAccessibility, hasScrollToIndexAction
#[test]
fn key_matchers_read_the_keys() {
    let mut ui = compose_test(|| {
        Column!({
            Box!(
                modifier = Modifier.test_tag("heading").semantics(|config| {
                    config.heading();
                    config.hide_from_accessibility();
                    config.on_ime_action(compose_ui::ImeAction::Search, None, || true);
                    config.scroll_to_index(None, |_| true);
                })
            );
            Box!(modifier = Modifier.test_tag("plain").size(1.dp()));
        });
    });
    ui.on_node_with_tag("heading")
        .assert(is_heading())
        .assert(is_hidden_from_accessibility())
        .assert(has_ime_action(compose_ui::ImeAction::Search))
        .assert(has_perform_ime_action())
        .assert(has_scroll_to_index_action())
        .assert(!has_scroll_to_key_action())
        .assert(key_is_defined(&SemanticsProperties::HEADING))
        .assert(key_not_defined(&SemanticsProperties::DISABLED));
    ui.on_node_with_tag("plain")
        .assert(!is_heading())
        .assert(key_not_defined(&SemanticsProperties::HEADING));
    assert_eq!(expect_value(&SemanticsProperties::FOCUSED, true).description, "Focused = 'true'");
    assert_eq!(key_is_defined(&SemanticsProperties::HEADING).description, "Heading is defined");
    assert_eq!(
        key_not_defined(&SemanticsProperties::HEADING).description,
        "Heading is NOT defined"
    );
    let message = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ui.on_node_with_tag("plain").assert(expect_value(&SemanticsProperties::FOCUSED, true));
    }))
    .expect_err("not focused")
    .downcast_ref::<String>()
    .cloned()
    .unwrap_or_default();
    assert!(message.contains("Focused = 'true'"), "{message}");
}

/// A clickable's semantics, as Compose's: a click (refused when disabled), the focus when
/// enabled, else disabled; printed as Compose prints them.
#[test]
fn a_clickables_semantics_are_composes() {
    let mut ui = compose_test(|| {
        Column!({
            Box!(modifier = Modifier.clickable(|| {}).size(10.dp()).test_tag("enabled"));
            Box!(
                modifier = Modifier
                    .clickable_with(
                        compose_ui::ClickableOptions {
                            enabled: false,
                            ..compose_ui::ClickableOptions::DEFAULT
                        },
                        || {},
                    )
                    .size(10.dp())
                    .test_tag("disabled")
            );
        });
    });
    ui.on_node_with_tag("enabled")
        .assert(has_click_action())
        .assert(is_focusable())
        .assert(is_not_focused())
        .assert(has_request_focus_action())
        .assert(is_enabled());
    ui.on_node_with_tag("disabled")
        .assert(has_click_action())
        .assert(is_not_focusable())
        .assert(!has_request_focus_action())
        .assert(is_not_enabled());
    let disabled = ui.on_node_with_tag("disabled").node_id();
    assert!(
        !ui.host().perform_semantics_action(disabled, &SemanticsActions::ON_CLICK),
        "a disabled click is refused"
    );
    let printed = ui.on_node_with_tag("enabled").print_to_string(0);
    assert!(printed.contains("Focused = 'false'"), "{printed}");
    assert!(printed.contains("Actions = [OnClick, RequestFocus]"), "{printed}");
    assert!(printed.contains("MergeDescendants = 'true'"), "{printed}");
    let printed = ui.on_node_with_tag("disabled").print_to_string(0);
    assert!(printed.contains("[Disabled]"), "{printed}");
}
