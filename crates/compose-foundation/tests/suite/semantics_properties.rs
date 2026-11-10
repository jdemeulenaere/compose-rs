//! Semantics as Compose's typed keys (D1250): the merged configuration of a node, each key's
//! merge policy, actions collapsed on one node and merged from descendants, an application's
//! own keys and actions, and the key matchers.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    AccessibilityAction, ActionFn, CustomAccessibilityAction, Delegates, FillableData, Layout,
    MeasurePolicy, ModifierNode, ModifierNodeElement, SemanticsActions, SemanticsConfiguration,
    SemanticsModifierNode, SemanticsProperties, SemanticsPropertyKey, SemanticsValue, SharedString,
    StringList, UnitExt, measure_policy,
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
    let unmerged = ui.on_node_with_tag_unmerged(tag).fetch_semantics_snapshot().config;
    let merged = ui.on_node_with_tag(tag).fetch_semantics_snapshot().config;
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
    let unmerged = ui.on_node_with_tag_unmerged("pane").fetch_semantics_snapshot().config;
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
        ui.on_node_with_tag("clickable").fetch_semantics_snapshot();
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
    let action = ui.on_node_with_tag("action").fetch_semantics_snapshot().config;
    assert_eq!(
        label_and_result(&action, &SemanticsActions::COPY_TEXT),
        (Some("copy".to_string()), Some(true))
    );
    let label = ui.on_node_with_tag("label").fetch_semantics_snapshot().config;
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
        let config = ui.on_node_with_tag("button").fetch_semantics_snapshot().config;
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
    let action = ui.on_node_with_tag("action").fetch_semantics_snapshot().config;
    assert_eq!(
        label_and_result(&action, &SemanticsActions::EXPAND),
        (Some("show more".to_string()), Some(true))
    );
    let label = ui.on_node_with_tag("label").fetch_semantics_snapshot().config;
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
        let config = ui.on_node_with_tag("parent").fetch_semantics_snapshot().config;
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

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/semantics/SemanticsConfiguration.kt, set (an AccessibilityAction over one already set)
/// An application's action collapses over an earlier one on a node as Compose's own do: the
/// outer's label, the inner's function.
#[test]
fn an_applications_action_collapses_on_a_node() {
    let mut inner = SemanticsConfiguration::default();
    inner.set(
        &SET_STRING,
        AccessibilityAction::new(None, Rc::new(|value: String| value == "yes") as Rc<SetStringFn>),
    );
    let mut outer = SemanticsConfiguration::default();
    outer.set(&SET_STRING, AccessibilityAction { label: Some("say".into()), action: None });
    inner.apply(&outer);
    let action = inner.get(&SET_STRING).expect("the action");
    assert_eq!(action.label.as_deref(), Some("say"));
    assert!(action.action.as_ref().is_some_and(|action| action("yes".to_string())));
}

/// A type with an `as_any` of its own, under the glob imports apps use: the semantics'
/// helpers don't clash with it (they're named apart and not exported at the root).
mod an_apps_own_as_any {
    use compose_ui::*;

    trait AsAny {
        fn as_any(&self) -> &dyn std::any::Any;
    }

    #[derive(Clone, PartialEq, Debug)]
    struct Mine;

    impl AsAny for Mine {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    impl SemanticsValue for Mine {}

    #[test]
    fn an_apps_as_any_is_its_own() {
        assert!(Mine.as_any().is::<Mine>());
        let _ = Modifier;
    }
}

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

/// A clickable's semantics, as Compose's: a click (called even when disabled, D1255), the focus when
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
        ui.host().perform_semantics_action(disabled, &SemanticsActions::ON_CLICK),
        "a disabled click is called, as Compose's (the bridges refuse it, D1255)"
    );
    let printed = ui.on_node_with_tag("enabled").print_to_string(0);
    assert!(printed.contains("Focused = 'false'"), "{printed}");
    assert!(printed.contains("Actions = [OnClick, RequestFocus]"), "{printed}");
    assert!(printed.contains("MergeDescendants = 'true'"), "{printed}");
    let printed = ui.on_node_with_tag("disabled").print_to_string(0);
    assert!(printed.contains("[Disabled]"), "{printed}");
}

// Compose: compose/ui/ui-test/src/commonMain/kotlin/androidx/compose/ui/test/SemanticsMatcher.kt, expectValue
/// `expect_value` checks the value, not only the key: an unfocused focusable isn't
/// `Focused = 'true'`, a button isn't a tab.
#[test]
fn expect_value_compares_the_value() {
    let mut ui = compose_test(|| {
        Box!(
            modifier = Modifier
                .clickable_with(
                    compose_ui::ClickableOptions {
                        role: Some(compose_ui::Role::Button),
                        ..compose_ui::ClickableOptions::DEFAULT
                    },
                    || {},
                )
                .size(10.dp())
                .test_tag("button")
        );
    });
    let mut matches = |matcher: Matcher| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ui.on_node_with_tag("button").assert(matcher);
        }))
        .is_ok()
    };
    assert!(matches(expect_value(&SemanticsProperties::FOCUSED, false)));
    assert!(!matches(expect_value(&SemanticsProperties::FOCUSED, true)));
    assert!(matches(expect_value(&SemanticsProperties::ROLE, compose_ui::Role::Button)));
    assert!(!matches(expect_value(&SemanticsProperties::ROLE, compose_ui::Role::Tab)));
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ui.on_node_with_tag("button")
            .assert(expect_value(&SemanticsProperties::ROLE, compose_ui::Role::Tab));
    }));
    let message = failed.expect_err("a button isn't a tab");
    let message = message.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(message.contains("Role = 'Tab'"), "{message}");
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/semantics/SemanticsProperties.kt, IsDialog
#[test]
#[should_panic(
    expected = "merge function called on unmergeable property IsDialog. A dialog should not be a \
                child of a clickable/focusable node."
)]
fn a_dialog_does_not_merge() {
    SemanticsProperties::IS_DIALOG.merge(None, &());
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/semantics/SemanticsProperties.kt, IsPopup
#[test]
#[should_panic(
    expected = "merge function called on unmergeable property IsPopup. A popup should not be a \
                child of a clickable/focusable node."
)]
fn a_popup_does_not_merge() {
    SemanticsProperties::IS_POPUP.merge(None, &());
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/semantics/SemanticsProperties.kt, HideFromAccessibility, Shape, BackgroundColor
/// Hiding, the shape and the background color never merge up: the parent's, else none.
#[test]
fn hiding_the_shape_and_the_background_keep_the_parents() {
    use compose_ui::{ColorProducer, Shape};
    let hide = &SemanticsProperties::HIDE_FROM_ACCESSIBILITY;
    assert_eq!(hide.merge(None, &()), None);
    assert_eq!(hide.merge(Some(&()), &()), Some(()));
    let shape = &SemanticsProperties::SHAPE;
    assert_eq!(shape.merge(None, &Shape::Rectangle), None);
    let circle = Shape::Rounded([compose_ui::CornerSize::Percent(50.0); 4]);
    assert_eq!(shape.merge(Some(&circle), &Shape::Rectangle), Some(circle));
    let background = &SemanticsProperties::BACKGROUND_COLOR;
    let red = ColorProducer(Rc::new(|| compose_ui::Color::RED));
    let blue = ColorProducer(Rc::new(|| compose_ui::Color::BLUE));
    assert_eq!(background.merge(None, &red), None);
    assert_eq!(background.merge(Some(&blue), &red), Some(blue));
}

// ---- SemanticsTests: merging and clearing through layouts -------------------------------------

/// Compose's `TestProperty`: merged, the parent's value, a comma, the child's.
fn concatenate(parent: Option<&SharedString>, child: &SharedString) -> Option<SharedString> {
    Some(match parent {
        None => child.clone(),
        Some(parent) => SharedString::from(format!("{parent}, {child}")),
    })
}

static TEST_PROPERTY: SemanticsPropertyKey<SharedString> =
    SemanticsPropertyKey::with_merge_policy("TestProperty", concatenate);

fn test_property(value: &'static str) -> Modifier {
    Modifier.semantics(move |config| config.set(&TEST_PROPERTY, SharedString::from(value)))
}

fn merging() -> Modifier {
    Modifier.semantics_with(true, |_| {})
}

fn test_property_equals(value: &str) -> Matcher {
    expect_value(&TEST_PROPERTY, SharedString::from(value.to_string()))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, depthFirstPropertyConcat
/// A merging node takes its descendants' values depth first (Compose's `SimpleTestLayout`, which
/// stacks its children at the origin, is a `Box` here).
#[test]
fn a_merged_value_concatenates_depth_first() {
    let mut ui = compose_test(|| {
        Box!(
            modifier = Modifier
                .test_tag("tag")
                .semantics_with(true, |config| config.set(&TEST_PROPERTY, "root".into())),
            {
                Box!(modifier = test_property("child1"), {
                    Box!(modifier = test_property("grandchild1"));
                    Box!(modifier = test_property("grandchild2"));
                });
                // Compose's `child2` is "grandchild2" too.
                Box!(modifier = test_property("grandchild2"));
            }
        );
    });
    ui.on_node_with_tag("tag")
        .assert(test_property_equals("root, child1, grandchild1, grandchild2, grandchild2"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, nestedMergedSubtree, nestedMergedSubtree_includeAllMergeableChildren
/// A merging node inside a merging one keeps its subtree's values; the outer one takes the rest.
#[test]
fn a_nested_merging_node_keeps_its_own_subtree() {
    for with_third in [false, true] {
        let mut ui = compose_test(move || {
            Box!(modifier = merging().test_tag("tag1"), {
                Box!(modifier = test_property("foo"));
                Box!(modifier = merging().test_tag("tag2"), {
                    Box!(modifier = test_property("bar"));
                });
                if with_third {
                    Box!(modifier = test_property("hi"));
                }
            });
        });
        let outer = if with_third { "foo, hi" } else { "foo" };
        ui.on_node_with_tag("tag1").assert(test_property_equals(outer));
        ui.on_node_with_tag("tag2").assert(test_property_equals("bar"));
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, clearAndSetSemantics
/// A clearing node hides its subtree from the merging node above, which takes only what the
/// clearing node sets; a merging node that clears its own subtree keeps what it sets.
#[test]
fn a_clearing_node_gives_a_merging_parent_only_its_own() {
    let mut ui = compose_test(|| {
        Box!(modifier = merging().test_tag("tag1"), {
            Box!(modifier = test_property("foo"));
            Box!(modifier = Modifier.clear_and_set_semantics(|_| {}), {
                Box!(modifier = test_property("hidden"));
            });
            Box!(
                modifier = Modifier
                    .clear_and_set_semantics(|config| config.set(&TEST_PROPERTY, "baz".into())),
                {
                    Box!(modifier = test_property("hidden"));
                }
            );
            Box!(
                modifier = merging()
                    .test_tag("tag2")
                    .clear_and_set_semantics(|config| config.set_text("foo")),
                {
                    Box!(modifier = text("hidden"));
                }
            );
        });
    });
    ui.on_node_with_tag("tag1").assert(test_property_equals("foo, baz"));
    ui.on_node_with_tag("tag2").assert_text_equals(&["foo"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, clearAndSetSemantics_unmergedTree
/// A clearing node over a merging one has only its own value, the merging one's left out.
#[test]
fn a_clearing_node_over_a_merging_one_has_only_its_own() {
    let mut ui = compose_test(|| {
        Box!(
            modifier = Modifier
                .test_tag("tag1")
                .clear_and_set_semantics(|config| config.set(&TEST_PROPERTY, "bar".into())),
            {
                Box!(
                    modifier = Modifier
                        .semantics_with(true, |config| config.set(&TEST_PROPERTY, "foo".into())),
                    {
                        Box!(modifier = test_property("hidden"));
                    }
                );
            }
        );
    });
    ui.on_node_with_tag("tag1").assert(test_property_equals("bar"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, clearAndSetSemanticsSameLayoutNode
/// On one node, a clearing modifier clears the modifiers after it, not those before it.
#[test]
fn a_clearing_modifier_clears_the_inner_ones_of_its_node() {
    let mut ui = compose_test(|| {
        Box!(modifier = merging().test_tag("tag1"), {
            Box!(
                modifier = Modifier
                    .clear_and_set_semantics(|config| config.set(&TEST_PROPERTY, "foo".into()))
                    .then(text("hidden"))
            );
            Box!(
                modifier =
                    test_property("baz").clear_and_set_semantics(|config| config.set_text("baz"))
            );
        });
        Box!(
            modifier = Modifier
                .test_tag("tag2")
                .then(test_property("foo"))
                .clear_and_set_semantics(|_| {})
                .then(text("foo"))
        );
    });
    ui.on_node_with_tag("tag1").assert(test_property_equals("foo, baz"));
    ui.on_node_with_tag("tag1").assert_text_equals(&["baz"]);
    ui.on_node_with_tag("tag2").assert(test_property_equals("foo"));
    ui.on_node_with_tag("tag2").assert(key_not_defined(&SemanticsProperties::TEXT));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, clearAndSetSemantics_children
/// A clearing node's children are in the unmerged tree but not among its accessibility
/// children; merged, it has none.
#[test]
fn a_clearing_nodes_children_are_only_in_the_unmerged_tree() {
    let mut ui = compose_test(|| {
        Column!(
            modifier = Modifier
                .test_tag("tag")
                .semantics_with(true, |_| {})
                .clear_and_set_semantics(|_| {}),
            {
                let size = || Modifier.size(100.dp());
                Box!(modifier = size().semantics(|config| config.set_content_description("box 1")));
                Box!(
                    modifier = size()
                        .clear_and_set_semantics(|config| config.set_content_description("box 2")),
                    {}
                );
                Box!(
                    modifier = size()
                        .semantics_with(true, |config| config.set_content_description("box 3"))
                );
            }
        );
    });
    let unmerged = ui.on_node_with_tag_unmerged("tag").fetch_semantics_node();
    let mut children = Vec::new();
    unmerged.collect_accessibility_children(&mut children);
    assert!(children.is_empty());
    assert_eq!(unmerged.children().len(), 3);

    let merged = ui.on_node_with_tag("tag").fetch_semantics_node();
    let mut merged_children = Vec::new();
    merged.collect_accessibility_children(&mut merged_children);
    assert!(merged_children.is_empty());
    assert!(merged.children().is_empty());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, higherUpSemanticsOverridePropertiesOfLowerSemanticsOnSameNode
#[test]
fn an_outer_modifier_overrides_an_inner_ones_description() {
    let mut ui = compose_test(|| {
        Box!(
            modifier = Modifier
                .test_tag("tag")
                .semantics(|config| config.set_content_description("high"))
                .semantics(|config| config.set_content_description("low"))
        );
    });
    ui.on_node_with_tag("tag").assert(expect_value(
        &SemanticsProperties::CONTENT_DESCRIPTION,
        StringList::from_iter([SharedString::from("high")]),
    ));
}

// ---- SemanticsTests: updates ------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, removingMergedSubtree_updatesSemantics
#[test]
fn a_merged_subtree_removed_leaves_the_merging_node() {
    let show = mutable_state_of(true);
    let handle = show.handle();
    let mut ui = compose_test(move || {
        Box!(modifier = merging().test_tag("tag"), {
            if handle.get() {
                Box!(modifier = test_property("foo"));
            }
        });
    });
    ui.on_node_with_tag("tag").assert(test_property_equals("foo"));
    ui.run_on_idle(|_| show.set(false));
    ui.on_node_with_tag("tag").assert(key_not_defined(&TEST_PROPERTY));
    ui.on_all_nodes_with_text("foo").assert_count_equals(0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, addingNewMergedNode_updatesSemantics
#[test]
fn a_node_added_under_a_merging_one_merges_into_it() {
    let show = mutable_state_of(false);
    let handle = show.handle();
    let mut ui = compose_test(move || {
        Box!(modifier = merging().test_tag("tag"), {
            Box!(modifier = test_property("foo"));
            if handle.get() {
                Box!(modifier = Modifier.semantics(|config| config.set_state_description("bar")));
            }
        });
    });
    ui.on_node_with_tag("tag")
        .assert(test_property_equals("foo"))
        .assert(key_not_defined(&SemanticsProperties::STATE_DESCRIPTION));
    ui.run_on_idle(|_| show.set(true));
    ui.on_node_with_tag("tag").assert(test_property_equals("foo")).assert_value_equals("bar");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, removingSubtreeWithoutSemanticsAsTopNode_updatesSemantics
#[test]
fn a_subtree_removed_under_a_node_without_semantics_leaves_the_tree() {
    let show = mutable_state_of(true);
    let handle = show.handle();
    let mut ui = compose_test(move || {
        Box!(modifier = Modifier.test_tag("tag"), {
            if handle.get() {
                Box!(modifier = Modifier.semantics(|config| config.set_content_description("foo")));
            }
        });
    });
    ui.on_all_nodes_with_content_description("foo").assert_count_equals(1);
    ui.run_on_idle(|_| show.set(false));
    ui.on_all_nodes_with_content_description("foo").assert_count_equals(0);
}

/// A description read from `after` when the semantics are computed, as Compose's
/// `if (isAfter.value) afterLabel else beforeLabel`.
fn before_or_after(after: MutableState<bool>) -> Modifier {
    Modifier.semantics(move |config| {
        config.set_content_description(if after.get() { "after" } else { "before" })
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, changingStackedSemanticsComponent_updatesSemantics, changingStackedSemanticsComponent_notTopMost_updatesSemantics, changingSemantics_belowStackedLayoutNodes_updatesCorrectly, updateToNodeWithMultipleBoundaryChildren_updatesCorrectly
/// A description changed by state shows on its node at the root, under a tagged node, under
/// two layouts, and over two children with semantics of their own.
#[test]
fn a_changed_description_shows_wherever_its_node_is() {
    for place in 0..4 {
        let after = mutable_state_of(false);
        let handle = after.handle();
        let mut ui = compose_test(move || {
            let node =
                move || Box!(modifier = Modifier.test_tag("tag").then(before_or_after(handle)));
            match place {
                0 => node(),
                1 => Box!(modifier = Modifier.test_tag("don't care"), {
                    node();
                }),
                2 => Box!({
                    Box!({
                        node();
                    });
                }),
                _ => Box!(modifier = Modifier.test_tag("tag").then(before_or_after(handle)), {
                    Box!(modifier = Modifier.semantics(|_| {}));
                    Box!(modifier = Modifier.semantics(|_| {}));
                }),
            }
        });
        ui.on_node_with_tag("tag").assert_content_description_equals(&["before"]);
        ui.run_on_idle(|_| after.set(true));
        ui.on_node_with_tag("tag").assert_content_description_equals(&["after"]);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, changingSemantics_belowNodeMergedThroughBoundary_updatesCorrectly
#[test]
fn a_value_changed_below_a_merging_node_shows_in_it() {
    let after = mutable_state_of(false);
    let handle = after.handle();
    let mut ui = compose_test(move || {
        Box!(modifier = Modifier.test_tag("tag").then(merging()), {
            Box!(
                modifier = Modifier.semantics(move |config| {
                    let value = if handle.get() { "after" } else { "before" };
                    config.set(&TEST_PROPERTY, value.into());
                })
            );
        });
    });
    ui.on_node_with_tag("tag").assert(test_property_equals("before"));
    ui.run_on_idle(|_| after.set(true));
    ui.on_node_with_tag("tag").assert(test_property_equals("after"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, mergeDescendants_doesNotCrossLayoutNodesUpward
#[test]
fn merging_does_not_reach_up_past_the_merging_node() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.test_tag("tag"), {
            Box!(modifier = merging(), {
                Box!(
                    modifier = Modifier.semantics(|config| config.set_content_description("label"))
                );
            });
        });
    });
    ui.on_node_with_tag("tag").assert(key_not_defined(&SemanticsProperties::CONTENT_DESCRIPTION));
    ui.on_node_with_content_description("label").assert_exists();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, changingSemantics_doesNotReplaceNodesBelow
/// Semantics changed by state don't compose the content below again (b/148606417).
#[test]
fn changed_semantics_do_not_compose_the_content_below_again() {
    let after = mutable_state_of(false);
    let handle = after.handle();
    let compositions = Rc::new(Cell::new(0));
    let counted = compositions.clone();
    // compose-rs runs a semantics block as the modifier is made, in composition: the state read
    // there composes the root again, where Compose's block reads outside composition. The
    // content below is skipped as Compose's is: a composable function given the same callback.
    let content = Callback::new(move || counted.set(counted.get() + 1));
    let mut ui = compose_test(move || {
        let content = content.clone();
        Box!(
            modifier = Modifier.test_tag("tag").semantics(move |config| {
                config.set_content_description(if handle.get() { "after" } else { "before" });
                config.on_click(None, move || {
                    let _ = handle.get();
                    true
                });
            }),
            {
                SimpleTestLayout(content.clone());
            }
        );
    });
    ui.on_node_with_tag("tag").assert_content_description_equals(&["before"]);
    assert_eq!(compositions.get(), 1);
    // Not Compose's: the tagged node keeps its identity too.
    let id = ui.on_node_with_tag("tag").fetch_semantics_node().id();
    ui.run_on_idle(|_| after.set(true));
    ui.on_node_with_tag("tag").assert_content_description_equals(&["after"]);
    assert_eq!(compositions.get(), 1);
    assert_eq!(ui.on_node_with_tag("tag").fetch_semantics_node().id(), id);
}

/// Compose's `SimpleTestLayout { content() }`, as its own composable function.
#[composable]
fn SimpleTestLayout(content: Callback) {
    Box!(move |_| content());
}

// ---- SemanticsTests: when semantics are computed ----------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, unchangedSemanticsDoesNotCauseRelayout
/// Composed again with the same semantics, a layout isn't measured again.
#[test]
fn unchanged_semantics_do_not_measure_again() {
    let forcer = mutable_state_of(0);
    let handle = forcer.handle();
    let measures = Rc::new(Cell::new(0));
    let counted = measures.clone();
    // Not Compose's: the content is composed again, so the count below isn't vacuous.
    let compositions = Rc::new(Cell::new(0));
    let composed = compositions.clone();
    let mut ui = compose_test(move || {
        handle.get();
        composed.set(composed.get() + 1);
        let counted = counted.clone();
        // Compose's `CountingLayout`: a remembered policy counting its measurements.
        let policy = remember(move || {
            let counted = counted.clone();
            measure_policy(move |scope, _, constraints| {
                counted.set(counted.get() + 1);
                scope.layout(constraints.min_width, constraints.min_height)
            }) as Rc<dyn MeasurePolicy>
        });
        Layout(
            Modifier.semantics(|config| config.set_content_description("label")),
            policy,
            callback(|| {}),
        );
    });
    ui.run_on_idle(|_| assert_eq!(measures.get(), 1));
    ui.run_on_idle(|_| forcer.set(forcer.get() + 1));
    ui.run_on_idle(|_| assert_eq!(measures.get(), 1));
    assert_eq!(compositions.get(), 2, "the forcer composes the content again");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, testSemanticsCalculatedOncePerComposition
/// Each semantics block runs once per composition (compose-rs runs it as its modifier is made,
/// Compose's eagerly with autofill on).
#[test]
fn semantics_are_computed_once_per_composition() {
    let count = Rc::new(Cell::new(0));
    let scope: Rc<Cell<Option<RecomposeScope>>> = Rc::default();
    let (counted, s) = (count.clone(), scope.clone());
    let mut ui = compose_test(move || {
        s.set(Some(current_recompose_scope()));
        let counting = |modifier: Modifier| {
            let counted = counted.clone();
            modifier.semantics(move |_| counted.set(counted.get() + 1))
        };
        Box!(modifier = counting(counting(counting(Modifier))));
    });
    ui.run_on_idle(|_| {
        assert_eq!(count.get(), 3);
        count.set(0);
        scope.get().expect("composed").invalidate();
    });
    ui.run_on_idle(|_| assert_eq!(count.get(), 3));
}

/// Compose's `SemanticsMod`: a node setting what its block sets.
struct SemanticsMod(Box<dyn Fn(&mut SemanticsConfiguration)>);

impl ModifierNode for SemanticsMod {
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl SemanticsModifierNode for SemanticsMod {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        (self.0)(config);
    }
}

/// Compose's `elementFor(object : DelegatingNode() { … })`: a node delegating to what `delegate`
/// makes.
#[derive(Clone)]
struct DelegatingElement(fn(&mut Delegates));

struct DelegatingNode(Delegates);

impl ModifierNodeElement for DelegatingElement {
    type Node = DelegatingNode;
    fn create(&self) -> DelegatingNode {
        let mut delegates = Delegates::new();
        (self.0)(&mut delegates);
        DelegatingNode(delegates)
    }
    fn update(&self, _: &mut DelegatingNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for DelegatingNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.0)
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, delegatedSemanticsPropertiesGetRead
#[test]
fn a_delegated_nodes_semantics_are_read() {
    let mut ui = compose_test(|| {
        let element = DelegatingElement(|delegates| {
            delegates.delegate(SemanticsMod(Box::new(|config| {
                config.set_content_description("hello world")
            })));
        });
        Box!(modifier = Modifier.test_tag("tag").then_custom(element));
    });
    ui.on_node_with_tag("tag").assert_content_description_equals(&["hello world"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, multipleDelegatesGetCombined
#[test]
fn several_delegates_semantics_are_combined() {
    let mut ui = compose_test(|| {
        let element = DelegatingElement(|delegates| {
            delegates.delegate(SemanticsMod(Box::new(|config| {
                config.set_content_description("hello world")
            })));
            delegates.delegate(SemanticsMod(Box::new(|config| {
                config.set(&TEST_PROPERTY, "bar".into())
            })));
        });
        Box!(modifier = Modifier.test_tag("tag").then_custom(element));
    });
    ui.on_node_with_tag("tag")
        .assert_content_description_equals(&["hello world"])
        .assert(test_property_equals("bar"));
}

// ---- SemanticsTests: autofill and moves -------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, fillableDataProperty
#[test]
fn fillable_data_is_on_the_node_merged_or_not() {
    let mut ui = compose_test(|| {
        Box!(
            modifier = Modifier.test_tag("tag").semantics(|config| {
                if let Some(data) = FillableData::create_from_text("foo") {
                    config.set_fillable_data(data);
                }
            })
        );
    });
    let fillable = || {
        Matcher::new("fillableData", |config| {
            config
                .get(&SemanticsProperties::FILLABLE_DATA)
                .and_then(FillableData::text_value)
                .is_some_and(|text| text.as_str() == "foo")
        })
    };
    ui.on_node_with_tag_unmerged("tag").assert(fillable());
    ui.on_node_with_tag("tag").assert(fillable());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, onFillDataAction
#[test]
fn the_fill_data_action_receives_the_data() {
    let received: Rc<RefCell<Option<FillableData>>> = Rc::default();
    let receiving = received.clone();
    let mut ui = compose_test(move || {
        let receiving = receiving.clone();
        Box!(
            modifier = Modifier.test_tag("tag").semantics(move |config| {
                config.on_fill_data(Some("fill".into()), move |data| {
                    *receiving.borrow_mut() = Some(data);
                    true
                });
            })
        );
    });
    let data = FillableData::create_from_text("foo").expect("always made here");
    let given = data.clone();
    ui.on_node_with_tag("tag").perform_semantics_action_with(
        &SemanticsActions::ON_FILL_DATA,
        move |action| {
            action(given);
        },
    );
    let text = received.borrow().as_ref().and_then(|data| data.text_value().cloned());
    assert_eq!(text.as_deref(), Some("foo"));
    let fill = move || {
        let data = data.clone();
        Matcher::new("fill", move |config| {
            config.get(&SemanticsActions::ON_FILL_DATA).is_some_and(|action| {
                action.label.as_deref() == Some("fill")
                    && action.action.as_ref().is_some_and(|action| action(data.clone()))
            })
        })
    };
    ui.on_node_with_tag_unmerged("tag").assert(fill());
    ui.on_node_with_tag("tag").assert(fill());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, testSemanticsUpdatedWhenMovedBetweenLayouts
/// Movable content moved to another parent is that parent's child in the tree.
#[test]
fn movable_content_moved_is_under_its_new_parent() {
    let moved = mutable_state_of(false);
    let handle = moved.handle();
    let mut ui = compose_test(move || {
        let content = remember(|| {
            movable_content_of(|| {
                Box!(
                    modifier = Modifier
                        .size(50.dp())
                        .semantics(|config| config.set_content_description("Movable Box"))
                        .test_tag("semantics_child")
                );
            })
        });
        Box!({
            let content = content.clone();
            if handle.get() {
                Box!(modifier = Modifier.test_tag("slot_1_parent"), {
                    content();
                });
            } else {
                Box!(modifier = Modifier.test_tag("slot_0_parent"), {
                    content();
                });
            }
        });
    });
    let child_of =
        |parent: &str| has_test_tag("semantics_child").and(has_parent(has_test_tag(parent)));
    ui.on_node_with_tag("slot_0_parent").assert_exists();
    ui.on_node_with_tag("slot_1_parent").assert_does_not_exist();
    ui.on_node(child_of("slot_0_parent")).assert_exists();
    ui.on_node(child_of("slot_1_parent")).assert_does_not_exist();

    ui.run_on_idle(|_| moved.set(true));

    ui.on_node_with_tag("slot_1_parent").assert_exists();
    ui.on_node_with_tag("slot_0_parent").assert_does_not_exist();
    ui.on_node(child_of("slot_1_parent")).assert_exists();
    ui.on_node(child_of("slot_0_parent")).assert_does_not_exist();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, testGetTextSizeFromTextLayoutResult
/// A text's layout result, through its semantics, has the density and the font size it was laid
/// out with (Material's `Text` is `BasicText` here).
#[test]
fn a_texts_layout_result_has_its_density_and_font_size() {
    let density = Rc::new(Cell::new(None));
    let seen = density.clone();
    // A device's density, as Compose's test runs at: at compose-rs's default of 1, a layout
    // input with the default density would pass.
    let root_density = compose_ui::Density::new(2.625);
    let measurer = Box::new(compose_ui::platform::FixedTextMeasurer::new());
    let mut ui = compose_test_with(DEFAULT_SIZE, root_density, measurer, move || {
        seen.set(Some(compose_ui::LOCAL_DENSITY.current()));
        let style = compose_ui::TextStyle { font_size: 14.sp(), ..compose_ui::TextStyle::DEFAULT };
        BasicText!("hello", modifier = Modifier.test_tag("tag"), style = style);
    });
    let mut results = Vec::new();
    ui.on_node_with_tag_unmerged("tag")
        .perform_semantics_action_with(&SemanticsActions::GET_TEXT_LAYOUT_RESULT, |get| {
            assert!(get(&mut results))
        });
    let input = &results[0].layout_input;
    assert_eq!(density.get(), Some(root_density));
    assert_eq!(Some(input.density), density.get());
    assert_eq!(input.style.font_size.value(), 14.0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, testSetTextSubstitution_annotatedString, testSetTextSubstitution_simpleString
/// A text's substitution set, then shown: its text stays, the substitution and whether it's
/// shown are on its node.
#[test]
#[ignore = "BasicText offers no text substitution actions (Compose's TextAnnotatedStringNode sets \
            SetTextSubstitution, ShowTextSubstitution, ClearTextSubstitution)"]
fn a_texts_substitution_is_set_then_shown() {
    for annotated in [true, false] {
        let mut ui = compose_test(move || {
            if annotated {
                BasicAnnotatedText!(
                    AnnotatedString::new("hello"),
                    modifier = Modifier.test_tag("tag")
                );
            } else {
                BasicText!("hello", modifier = Modifier.test_tag("tag"));
            }
        });
        fn ui_config(ui: &mut ComposeTestRule) -> SemanticsConfiguration {
            ui.on_node_with_tag_unmerged("tag").fetch_semantics_snapshot().config
        }
        let config = ui_config(&mut ui);
        assert_eq!(config.get(&SemanticsProperties::IS_SHOWING_TEXT_SUBSTITUTION), None);
        ui.on_node_with_tag_unmerged("tag").perform_semantics_action_with(
            &SemanticsActions::SET_TEXT_SUBSTITUTION,
            |set| {
                set(AnnotatedString::new("bonjour"));
            },
        );
        let check = |ui: &mut ComposeTestRule, showing: bool| {
            let config = ui_config(ui);
            assert_eq!(texts(&config), ["hello"], "the substitution doesn't change the text");
            assert_eq!(
                config.get(&SemanticsProperties::TEXT_SUBSTITUTION).map(|it| it.to_string()),
                Some("bonjour".to_string())
            );
            assert_eq!(
                config.get(&SemanticsProperties::IS_SHOWING_TEXT_SUBSTITUTION),
                Some(&showing)
            );
        };
        check(&mut ui, false);
        ui.on_node_with_tag_unmerged("tag").perform_semantics_action_with(
            &SemanticsActions::SHOW_TEXT_SUBSTITUTION,
            |show| {
                show(true);
            },
        );
        check(&mut ui, true);
    }
}
