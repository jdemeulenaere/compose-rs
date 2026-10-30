//! The semantics tree as live handles (D1287): layouts, parents and children in drawing order,
//! bounds, transparency, fake nodes, hit tests and what's left uncovered.

#![allow(non_snake_case)]

use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::*;

/// The handle of the node tagged `tag` in the unmerged tree.
fn node(ui: &mut ComposeTestRule, tag: &str) -> SemanticsNode {
    ui.wait_for_idle();
    let id = ui.on_node_with_tag_unmerged(tag).fetch_semantics_node().semantics_id;
    ui.host().semantics_owner().get(id).expect("attached, with semantics")
}

fn tag_of(config: &SemanticsConfiguration) -> Option<String> {
    config.get(&SemanticsProperties::TEST_TAG).map(|tag| tag.to_string())
}

/// Its tags, a layout with no semantics saying `None`.
fn tags_of_children(info: &LayoutInfo) -> Vec<Option<String>> {
    info.children_info()
        .iter()
        .map(|child| child.semantics_configuration().as_ref().and_then(tag_of))
        .collect()
}

fn descriptions(config: &SemanticsConfiguration) -> Vec<String> {
    let list = config.get(&SemanticsProperties::CONTENT_DESCRIPTION);
    list.into_iter().flatten().map(|description| description.to_string()).collect()
}

fn sized(size: i32) -> Modifier {
    Modifier.size((size as f32).dp())
}

// ---- Layouts ------------------------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsInfoTest.kt, contentWithNoSemantics
#[test]
fn a_root_with_no_semantics_has_a_layout_with_nothing_to_say() {
    let mut ui = compose_test(|| {
        Box!(modifier = sized(1), {});
    });
    ui.wait_for_idle();
    let root = ui.host().semantics_owner().root_info();
    assert_eq!(root.parent_info(), None);
    assert_eq!(tags_of_children(&root), [None]);
    assert_eq!(root.nearest_parent_that_has_semantics(), None);
    assert_eq!(root.find_merging_semantics_parent(), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsInfoTest.kt, singleSemanticsModifier, twoSemanticsModifiers
#[test]
fn layouts_with_semantics_know_their_root_and_their_siblings_order() {
    let mut ui = compose_test(|| {
        Box!(modifier = sized(1).test_tag("first"), {});
        Box!(modifier = sized(1).test_tag("second"), {});
    });
    let (first, second) =
        (node(&mut ui, "first").layout_info(), node(&mut ui, "second").layout_info());
    let root = ui.host().semantics_owner().root_info();
    assert_eq!(root.children_info(), [first, second]);
    assert_eq!(tags_of_children(&root), [Some("first".into()), Some("second".into())]);
    assert_eq!(first.parent_info(), Some(root));
    assert!(first.children_info().is_empty());
    assert_eq!(first.nearest_parent_that_has_semantics(), None, "the root says nothing here");
    assert_eq!(first.find_merging_semantics_parent(), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsInfoTest.kt, nodeDeepInHierarchy
#[test]
fn a_layout_deep_in_a_tree_finds_the_layouts_around_it_and_each_child_says_what_it_can() {
    let merged =
        |tag: &'static str| Modifier.semantics_with(true, move |config| config.set_test_tag(tag));
    let mut ui = compose_test(move || {
        Column!(modifier = merged("outer"), {
            Column!(modifier = merged("merging"), {
                Row!(modifier = Modifier.test_tag("row"), {
                    Column!({
                        Row!(modifier = Modifier.test_tag("target"), {
                            Box!({
                                Box!(modifier = Modifier.test_tag("first"), {});
                            });
                            Box!(modifier = Modifier.test_tag("second"), {
                                Box!(modifier = Modifier.test_tag("below"), {});
                            });
                            Box!({});
                        });
                    });
                });
            });
        });
    });
    let row = node(&mut ui, "row").layout_info();
    let merging = node(&mut ui, "merging").layout_info();
    let target = node(&mut ui, "target").layout_info();
    assert_ne!(target.parent_info(), Some(row), "a layout with nothing to say is between them");
    assert_eq!(target.nearest_parent_that_has_semantics(), Some(row));
    assert_eq!(target.find_merging_semantics_parent(), Some(merging));
    assert_eq!(tags_of_children(&target), [None, Some("second".into()), None]);
    assert_eq!(
        target.semantics_configuration().as_ref().and_then(tag_of).as_deref(),
        Some("target")
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsInfoTest.kt, readingSemanticsConfigurationOfDeactivatedNode
#[test]
fn a_layout_kept_for_reuse_is_deactivated_and_says_nothing() {
    let active = Box::leak(Box::new(mutable_state_of(true))).handle();
    let mut ui = compose_test(move || {
        ReusableContentHost(active.get(), || {
            Box!(modifier = sized(10).test_tag("kept"), {});
        });
    });
    let info = node(&mut ui, "kept").layout_info();
    assert!(!info.is_deactivated());
    assert!(info.semantics_configuration().is_some());
    active.set(false);
    ui.wait_for_idle();
    assert!(info.is_deactivated());
    assert!(info.semantics_configuration().is_none());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsInfoTest.kt, transparent, semiTransparent, nonTransparent, transparencyOfStackedItems
#[test]
fn a_layout_is_transparent_under_a_layer_with_no_alpha_only() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.alpha(1.0), {
            Box!(modifier = Modifier.test_tag("opaque"), {});
        });
        Box!(modifier = Modifier.alpha(0.5), {
            Box!(modifier = Modifier.test_tag("half"), {});
        });
        Box!(modifier = Modifier.alpha(0.0), {
            Box!(modifier = Modifier.test_tag("none"), {});
        });
    });
    assert!(!node(&mut ui, "opaque").layout_info().is_transparent());
    assert!(!node(&mut ui, "half").layout_info().is_transparent());
    assert!(node(&mut ui, "none").layout_info().is_transparent());
    assert_eq!(node(&mut ui, "half").compute_effective_alpha(), 0.5);
}

/// Measures its children and places none.
struct NotPlacing;

impl MeasurePolicy for NotPlacing {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        for child in measurables {
            scope.measure(*child, constraints);
        }
        scope.layout(5, 5)
    }
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/semantics/SemanticsHostApiTest.kt, placeOrderAndZIndex
#[test]
fn a_layout_knows_where_it_was_placed_and_its_z_index() {
    let mut ui = compose_test(|| {
        Box!(modifier = sized(30), {
            Box!(modifier = Modifier.test_tag("a"), {});
            Box!(modifier = Modifier.test_tag("b").z_index(2.0), {});
        });
        Layout(
            Modifier.test_tag("not placing"),
            Rc::new(NotPlacing),
            callback(|| {
                Box!(modifier = Modifier.test_tag("unplaced"), {});
            }),
        );
    });
    let (a, b) = (node(&mut ui, "a").layout_info(), node(&mut ui, "b").layout_info());
    assert_eq!((a.place_order(), b.place_order()), (0, 1));
    assert_eq!((a.z_index(), b.z_index()), (0.0, 2.0));
    let unplaced = node(&mut ui, "not placing").children();
    let unplaced = unplaced.first().expect("in the tree, placed or not").layout_info();
    assert!(!unplaced.is_placed());
    assert_eq!(unplaced.place_order(), LayoutInfo::NOT_PLACED_PLACE_ORDER);
}

// ---- Configurations -----------------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/semantics/SemanticsHostApiTest.kt, propertyQueries_unmergedAndMerged
#[test]
fn a_merging_node_declares_merging_but_only_merges_in_the_merged_tree() {
    let mut ui = compose_test(|| {
        let merging = Modifier
            .semantics_with(true, |config| config.set_content_description("parent"))
            .test_tag("merging");
        Box!(modifier = merging, {
            let child = Modifier.semantics(|config| config.set_state_description("state"));
            Box!(modifier = child.test_tag("child"), {});
        });
    });
    let merging = node(&mut ui, "merging");
    let unmerged = merging.unmerged_config();
    assert!(!unmerged.contains(&SemanticsProperties::STATE_DESCRIPTION));
    let mut keys = Vec::new();
    unmerged.for_each_property(|key, _| keys.push(key.name()));
    keys.sort();
    assert_eq!(keys, ["ContentDescription", "TestTag"]);
    assert!(unmerged.is_merging_semantics_of_descendants);
    assert!(!merging.is_merging_semantics_of_descendants(), "this is the unmerged tree");
    assert!(!merging.merging_enabled());

    let merged = merging.merged_config();
    assert_eq!(
        merged.get(&SemanticsProperties::STATE_DESCRIPTION).map(|it| it.to_string()).as_deref(),
        Some("state")
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/semantics/SemanticsHostApiTest.kt, getMergedOrNullAndForEachMergedProperty_matchMergedConfig
#[test]
fn what_a_node_merges_is_read_anew_each_time_and_stops_at_merging_and_clearing_nodes() {
    let state = Box::leak(Box::new(mutable_state_of("first"))).handle();
    let mut ui = compose_test(move || {
        let merging = Modifier
            .semantics_with(true, |config| config.set_content_description("parent"))
            .test_tag("merging");
        Box!(modifier = merging, {
            let child = Modifier.semantics(|config| {
                config.set_content_description("child");
                config.set_state_description(state.get());
            });
            Box!(modifier = child, {});
            Box!(
                modifier = Modifier.semantics_with(true, |config| config.set_pane_title("nested")),
                {}
            );
            Box!(modifier = Modifier.clear_and_set_semantics(|_| {}), {
                Box!(modifier = Modifier.semantics(|config| config.heading()), {});
            });
        });
    });
    let merging = node(&mut ui, "merging");
    let merged = merging.merged_config();
    assert_eq!(descriptions(&merged), ["parent", "child"]);
    assert_eq!(
        merging.get_merged_or_null(&SemanticsProperties::CONTENT_DESCRIPTION),
        merged.get(&SemanticsProperties::CONTENT_DESCRIPTION).cloned()
    );
    assert!(merging.get_merged_or_null(&SemanticsProperties::PANE_TITLE).is_none());
    assert!(merging.get_merged_or_null(&SemanticsProperties::HEADING).is_none());

    let mut visited = Vec::new();
    merging.for_each_merged_property(|key, _| visited.push(key.name()));
    assert!(visited.contains(&"ContentDescription") && visited.contains(&"StateDescription"));
    assert!(!visited.contains(&"PaneTitle") && !visited.contains(&"Heading"));

    let mut asked = 0;
    assert!(merging.any_merged_property(|_, _| {
        asked += 1;
        true
    }));
    assert_eq!(asked, 1, "the first says yes");
    assert!(merging.any_merged_property(|key, _| key.name() == "StateDescription"));
    assert!(!merging.any_merged_property(|key, _| key.name() == "PaneTitle"));

    state.set("second");
    ui.wait_for_idle();
    let read = |config: &SemanticsConfiguration| {
        config.get(&SemanticsProperties::STATE_DESCRIPTION).map(|it| it.to_string())
    };
    assert_eq!(read(&merging.merged_config()).as_deref(), Some("second"));
}

// ---- Children and parents -----------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, testChildrenAreZSorted
#[test]
fn children_are_in_drawing_order_in_the_handles_and_the_snapshots() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.test_tag("parent"), {
            Box!(modifier = Modifier.test_tag("over").z_index(1.0), {});
            Box!(modifier = Modifier.test_tag("under"), {});
        });
    });
    let parent = node(&mut ui, "parent");
    let tags: Vec<_> =
        parent.children().iter().map(|child| tag_of(&child.config()).unwrap()).collect();
    assert_eq!(tags, ["under", "over"]);
    let snapshot = ui.on_node_with_tag_unmerged("parent").fetch_semantics_node();
    let tags: Vec<_> =
        snapshot.children.iter().map(|child| tag_of(&child.config).unwrap()).collect();
    assert_eq!(tags, ["under", "over"]);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/semantics/SemanticsHostApiTest.kt, accessibilityChildren_fakeNodesClearAndSetAndLinkMarkers
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, collectAccessibilityChildren_includeFakeNodes, children_doNotIncludeFakeNodes, fakeSemanticsNode_usesValuesFromParent
#[test]
fn accessibility_children_add_fake_nodes_leave_markers_out_and_stop_at_a_clearing_node() {
    let mut ui = compose_test(|| {
        let button = Modifier
            .semantics_with(true, |config| {
                config.set_content_description("description");
                config.set_role(Role::Button);
            })
            .test_tag("button");
        Box!(modifier = button, {
            let child = Modifier.semantics(|config| config.set_state_description("state"));
            Box!(modifier = child.test_tag("child"), {});
            let marker =
                Modifier.semantics(|config| config.set(&SemanticsProperties::LINK_TEST_MARKER, ()));
            Box!(modifier = marker, {});
        });
        let cleared = Modifier
            .test_tag("cleared")
            .clear_and_set_semantics(|config| config.set_content_description("cleared"));
        Box!(modifier = cleared, {
            Box!(modifier = Modifier.semantics(|config| config.set_content_description("no")), {});
        });
    });
    let button = node(&mut ui, "button");
    let child = node(&mut ui, "child");
    let mut children = Vec::new();
    button.collect_accessibility_children(&mut children);
    let ids: Vec<_> = children.iter().map(SemanticsNode::id).collect();
    assert_eq!(
        ids,
        [
            button.id() + CONTENT_DESCRIPTION_FAKE_NODE_ID_OFFSET,
            child.id(),
            button.id() + ROLE_FAKE_NODE_ID_OFFSET,
        ]
    );
    assert_eq!(button.children().len(), 2, "the marker is a child; fake nodes aren't");
    assert!(button.has_accessibility_child());
    assert!(!child.has_accessibility_child());

    let mut appended = vec![child];
    button.collect_accessibility_children(&mut appended);
    assert_eq!(appended.len(), 4);
    assert_eq!(appended[0], child);

    let fake = children[0];
    assert!(fake.is_fake());
    assert_eq!(descriptions(&fake.config()), ["description"]);
    assert_eq!(fake.touch_bounds_in_root(), button.touch_bounds_in_root());
    assert_eq!(fake.bounds_in_root(), button.bounds_in_root());
    assert_eq!(fake.parent(), Some(button));
    assert!(children[2].is_fake());
    assert_eq!(children[2].role(), Some(Role::Button));

    let cleared = node(&mut ui, "cleared");
    assert!(!cleared.has_accessibility_child());
    assert_eq!(cleared.children().len(), 1, "the unmerged tree shows what it clears");
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/semantics/SemanticsHostApiTest.kt, forEachSemanticsAncestor_nearestFirst_fakeNodeVisitsOwnerFirst
#[test]
fn ancestors_are_visited_nearest_first_and_a_fake_nodes_owner_before_them() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.test_tag("outer"), {
            Box!({
                let button = Modifier
                    .semantics_with(true, |config| config.set_content_description("description"))
                    .test_tag("button");
                Box!(modifier = button, {
                    Box!(modifier = Modifier.test_tag("child"), {});
                });
            });
        });
    });
    let (outer, button, child) =
        (node(&mut ui, "outer"), node(&mut ui, "button"), node(&mut ui, "child"));
    let mut ancestors = Vec::new();
    let found = child.for_each_semantics_ancestor(|info, _| {
        ancestors.push(info.semantics_id());
        false
    });
    assert_eq!(found, None);
    assert_eq!(ancestors[..2], [button.id(), outer.id()]);

    let mut children = Vec::new();
    button.collect_accessibility_children(&mut children);
    let fake = children.iter().find(|it| it.is_fake()).expect("its description");
    let mut merges = false;
    let first = fake.for_each_semantics_ancestor(|_, config| {
        merges = config.is_merging_semantics_of_descendants;
        true
    });
    assert_eq!(first.map(|info| info.semantics_id()), Some(button.id()));
    assert!(merges);
}

#[test]
fn parents_are_the_closest_merging_node_in_the_merged_tree_and_the_closest_node_otherwise() {
    let mut ui = compose_test(|| {
        let merging = Modifier
            .semantics_with(true, |config| config.set_content_description("merging"))
            .test_tag("merging");
        Box!(modifier = merging, {
            Box!(modifier = Modifier.test_tag("between"), {
                let inner = Modifier.semantics_with(true, |config| config.heading());
                Box!(modifier = inner.test_tag("inner"), {});
            });
        });
    });
    let inner = node(&mut ui, "inner");
    assert_eq!(inner.parent(), Some(node(&mut ui, "between")));
    let merged_root = ui.host().semantics_owner().root_semantics_node();
    assert!(merged_root.is_root());
    let merging = merged_root.children()[0];
    assert!(merging.is_merging_semantics_of_descendants());
    let below = merging.children();
    assert_eq!(below.len(), 1, "only what merges itself shows");
    assert_eq!(below[0].id(), inner.id());
    assert_eq!(below[0].parent(), Some(merging));
    assert_eq!(merging.parent(), Some(merged_root));
    assert_eq!(merged_root.parent(), None);
}

// ---- Bounds -------------------------------------------------------------------------------------

/// Padding and a size, the node's semantics inside the padding.
fn padded(padding: i32, size: i32, semantics: Modifier) -> Modifier {
    sized(size + 2 * padding).padding((padding as f32).dp()).then(semantics)
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/semantics/SemanticsHostApiTest.kt, boundsQueries
#[test]
fn bounds_are_in_the_parent_and_the_root_each_from_what_says_so() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        Box!(modifier = padded(5, 40, Modifier.test_tag("parent")), {
            Box!(modifier = padded(3, 10, Modifier.test_tag("child")), {});
        });
    });
    let (parent, child) = (node(&mut ui, "parent"), node(&mut ui, "child"));
    assert_eq!(child.calculate_bounds_in_parent(), Rect::new(3.0, 3.0, 13.0, 13.0));
    assert_eq!(
        child.layout_info().calculate_outer_bounds_in_root(),
        Rect::new(5.0, 5.0, 21.0, 21.0)
    );
    assert_eq!(child.bounds_in_root(), Rect::new(8.0, 8.0, 18.0, 18.0));
    assert_eq!(child.position_in_root(), Offset::new(8.0, 8.0));
    assert_eq!(child.bounds_in_window(), child.bounds_in_root());
    assert_eq!(child.size(), IntSize::new(10, 10));
    assert_eq!(parent.calculate_bounds_in_parent(), Rect::new(5.0, 5.0, 45.0, 45.0));
}

/// A semantics node saying nothing, and not important for bounds.
struct NotImportantForBounds;

struct NotImportantForBoundsNode;

impl ModifierNodeElement for NotImportantForBounds {
    type Node = NotImportantForBoundsNode;
    fn create(&self) -> NotImportantForBoundsNode {
        NotImportantForBoundsNode
    }
    fn update(&self, _node: &mut NotImportantForBoundsNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "not_important_for_bounds"
    }
}

impl ModifierNode for NotImportantForBoundsNode {
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl SemanticsModifierNode for NotImportantForBoundsNode {
    fn apply_semantics(&self, _config: &mut SemanticsConfiguration) {}
    fn is_important_for_bounds(&self) -> bool {
        false
    }
}

/// A node at (20, 30) in a box with padding (10, 20): `around` on that box, after the padding.
fn nested_at_an_offset(around: Modifier) -> Rect {
    let around = Rc::new(around);
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let modifier = sized(100).padding((10.dp(), 20.dp())).then((*around).clone());
        Box!(modifier = modifier, {
            Box!(modifier = sized(10).offset(20.dp(), 30.dp()), {
                Box!(modifier = sized(1).test_tag("target"), {});
            });
        });
    });
    node(&mut ui, "target").calculate_bounds_in_parent()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, testBoundsInParent, testBoundsInParent_boundsInRootWhenNoParent, testBoundsInParent_boundsInRootWhenParentIsNotImportantForBounds
#[test]
fn bounds_in_a_parent_are_in_the_closest_with_bounds_or_the_root() {
    assert_eq!(
        nested_at_an_offset(Modifier.semantics(|_| {})),
        Rect::new(20.0, 30.0, 21.0, 31.0),
        "a modifier saying nothing still has bounds"
    );
    assert_eq!(nested_at_an_offset(Modifier), Rect::new(30.0, 50.0, 31.0, 51.0));
    assert_eq!(
        nested_at_an_offset(Modifier.then_custom(NotImportantForBounds)),
        Rect::new(30.0, 50.0, 31.0, 51.0)
    );
}

#[test]
fn a_clip_around_a_node_shows_in_its_bounds_and_not_where_it_is() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        Box!(modifier = sized(10).clip_to_bounds(), {
            Box!(modifier = Modifier.required_size(20.dp()).test_tag("clipped"), {});
        });
    });
    let clipped = node(&mut ui, "clipped");
    assert_eq!(clipped.size(), IntSize::new(20, 20));
    assert_eq!(clipped.unclipped_bounds_in_root().width(), 20.0);
    assert_eq!(clipped.bounds_in_root().width(), 10.0);
}

#[test]
fn a_clickable_small_node_takes_touches_over_the_minimum_target() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        Box!(modifier = Modifier.padding(50.dp()), {
            Box!(modifier = sized(10).clickable(|| {}).test_tag("small"), {});
            Box!(modifier = sized(10).test_tag("not clickable"), {});
        });
    });
    let small = node(&mut ui, "small");
    let touch = small.touch_bounds_in_root();
    assert_eq!((touch.width(), touch.height()), (48.0, 48.0));
    assert_eq!(touch.left, small.bounds_in_root().left - 19.0);
    let other = node(&mut ui, "not clickable");
    assert_eq!(other.touch_bounds_in_root(), other.bounds_in_root());
}

// ---- What accessibility reads -------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/semantics/SemanticsHostApiTest.kt, visibilityQueries
#[test]
fn a_node_is_hidden_when_transparent_or_hiding_and_important_when_it_says_what_accessibility_reads()
{
    let mut ui = compose_test(|| {
        let says = |config: &mut SemanticsConfiguration| config.set_content_description("says");
        Box!(modifier = Modifier.alpha(0.0), {
            Box!(modifier = Modifier.semantics(says).test_tag("transparent"), {});
        });
        Box!(modifier = Modifier.semantics(says).alpha(0.0).test_tag("alpha inside"), {});
        Box!(modifier = Modifier.semantics(says).test_tag("visible"), {});
        let hiding = |config: &mut SemanticsConfiguration| config.hide_from_accessibility();
        Box!(modifier = Modifier.semantics(hiding).test_tag("hiding"), {});
        Box!(modifier = Modifier.test_tag("tag only"), {});
        let link = Modifier.semantics(|config| {
            config.set_content_description("link");
            config.set(&SemanticsProperties::LINK_TEST_MARKER, ());
        });
        Box!(modifier = link.test_tag("link"), {});
    });
    let transparent = node(&mut ui, "transparent");
    assert!(transparent.layout_info().is_transparent());
    assert!(transparent.is_hidden());
    assert!(!transparent.is_important_for_accessibility());
    assert!(!node(&mut ui, "alpha inside").is_hidden(), "the layer is inside what says it");
    let visible = node(&mut ui, "visible");
    assert!(!visible.layout_info().is_transparent());
    assert!(!visible.is_hidden());
    assert!(visible.is_important_for_accessibility());
    assert!(node(&mut ui, "hiding").is_hidden());
    assert!(!node(&mut ui, "hiding").is_important_for_accessibility());
    assert!(!node(&mut ui, "tag only").is_important_for_accessibility());
    assert!(!node(&mut ui, "link").is_important_for_accessibility());
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/semantics/SemanticsHostApiTest.kt, hitTest_topMostFirst_earlyExit_lookups, hitTest_inlineLinkMarker_returnsTextNode
#[test]
fn a_hit_test_finds_the_top_most_first_and_stops_where_asked() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Box!(modifier = sized(200).test_tag("bottom"), {
            Box!(modifier = sized(10).test_tag("top"), {
                let marker = Modifier
                    .semantics(|config| config.set(&SemanticsProperties::LINK_TEST_MARKER, ()));
                Box!(modifier = sized(10).then(marker), {});
            });
        });
    });
    let (bottom, top) = (node(&mut ui, "bottom"), node(&mut ui, "top"));
    let owner = ui.host().semantics_owner();
    let mut hits = Vec::new();
    let found = owner.hit_test(Offset::new(5.0, 5.0), |info| {
        hits.push(info.semantics_id());
        false
    });
    assert_eq!(found, None);
    assert_eq!(hits[..2], [top.id(), bottom.id()], "the marker's left out");
    assert_eq!(
        owner.hit_test(Offset::new(150.0, 150.0), |_| true).map(|it| it.semantics_id()),
        Some(bottom.id())
    );
    let mut asked = 0;
    let first = owner.hit_test(Offset::new(5.0, 5.0), |_| {
        asked += 1;
        true
    });
    assert_eq!(asked, 1);
    assert_eq!(first.map(|it| it.semantics_id()), Some(top.id()));
    assert_eq!(owner.get(top.id()).map(|it| it.id()), Some(top.id()));
    assert!(!owner.get(top.id()).unwrap().merging_enabled());
    assert_eq!(owner.get_layout_info(top.id()).map(|it| it.semantics_id()), Some(top.id()));
    assert_eq!(owner.get(i32::MAX - 1), None);
    assert_eq!(owner.get_layout_info(i32::MAX - 1), None);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/semantics/SemanticsHostApiTest.kt, forEachUncoveredSemanticsNode_prunesCoveredNodes
#[test]
fn what_a_sibling_over_it_covers_completely_is_left_out_and_the_root_never_is() {
    let says = |config: &mut SemanticsConfiguration| config.set_content_description("says");
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        Box!({
            Box!(modifier = sized(10).semantics(says).test_tag("covered"), {});
            Box!(modifier = sized(20).semantics(says).test_tag("cover"), {});
        });
    });
    let (covered, cover) = (node(&mut ui, "covered"), node(&mut ui, "cover"));
    let owner = ui.host().semantics_owner();
    let root = owner.unmerged_root_semantics_node();
    let mut visited = Vec::new();
    owner.for_each_uncovered_semantics_node(|node, bounds, _| visited.push((node.id(), bounds)));
    assert!(visited.iter().any(|(id, _)| *id == root.id()));
    assert!(visited.contains(&(cover.id(), Rect::new(0.0, 0.0, 20.0, 20.0))));
    assert!(!visited.iter().any(|(id, _)| *id == covered.id()));
}

#[test]
fn what_is_left_of_a_node_partly_covered_is_what_its_bounds_say() {
    let says = |config: &mut SemanticsConfiguration| config.set_content_description("says");
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        Box!({
            Box!(modifier = Modifier.size((40.dp(), 10.dp())).semantics(says).test_tag("under"), {
            });
            Box!(modifier = Modifier.size((10.dp(), 10.dp())).semantics(says).test_tag("over"), {});
        });
    });
    let under = node(&mut ui, "under");
    let mut left = None;
    ui.host().semantics_owner().for_each_uncovered_semantics_node(|node, bounds, _| {
        if node.id() == under.id() {
            left = Some(bounds);
        }
    });
    assert_eq!(left, Some(Rect::new(10.0, 0.0, 40.0, 10.0)));
}

#[test]
fn a_dialogs_nodes_hang_from_the_root_and_merge_nothing_around_it() {
    let mut ui = compose_test(|| {
        let merging = Modifier
            .semantics_with(true, |config| config.set_content_description("around"))
            .test_tag("around");
        Box!(modifier = merging, {
            Dialog!(on_dismiss_request = || {}, {
                Box!(modifier = sized(10).test_tag("in dialog"), {});
            });
        });
    });
    let inside = node(&mut ui, "in dialog");
    let around = node(&mut ui, "around");
    assert!(!around.children().contains(&inside));
    let mut parent = inside.parent();
    while let Some(node) = parent.filter(|node| !node.is_root()) {
        assert_ne!(node, around);
        parent = node.parent();
    }
    assert_eq!(parent, Some(ui.host().semantics_owner().unmerged_root_semantics_node()));
}

#[test]
fn a_handle_of_content_gone_or_reused_answers_as_a_node_with_nothing() {
    let shown = Box::leak(Box::new(mutable_state_of(true))).handle();
    let mut ui = compose_test(move || {
        if shown.get() {
            Box!(modifier = Modifier.test_tag("gone").semantics(|config| config.heading()), {});
        }
    });
    let gone = node(&mut ui, "gone");
    assert!(gone.unmerged_config().contains(&SemanticsProperties::HEADING));
    shown.set(false);
    ui.wait_for_idle();
    assert!(gone.unmerged_config().is_empty());
    assert!(gone.children().is_empty());
    assert_eq!(gone.bounds_in_root(), Rect::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(gone.parent(), None);
}

#[test]
fn a_handle_of_a_node_reused_for_other_content_answers_as_a_node_with_nothing() {
    let content = Box::leak(Box::new(mutable_state_of(0))).handle();
    let mut ui = compose_test(move || {
        ReusableContent(content.get(), move || {
            Box!(modifier = Modifier.test_tag(format!("content {}", content.get())), {});
        });
    });
    let old = node(&mut ui, "content 0");
    content.set(1);
    ui.wait_for_idle();
    let new = node(&mut ui, "content 1");
    assert_ne!(old.id(), new.id());
    assert!(old.unmerged_config().is_empty(), "not the new content's");
    assert!(!old.layout_info().is_attached());
}

#[test]
fn a_hit_stops_what_is_under_it_and_a_clearing_node_what_is_below_it() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        Box!({
            Box!(modifier = sized(10).test_tag("under"), {});
            Box!(modifier = sized(10).test_tag("over"), {});
        });
        Box!(modifier = Modifier.offset(50.dp(), 0.dp()), {
            let clearing = sized(10).test_tag("clearing").clear_and_set_semantics(|_| {});
            Box!(modifier = clearing, {
                Box!(modifier = sized(10).test_tag("below"), {});
            });
        });
    });
    let (under, over) = (node(&mut ui, "under"), node(&mut ui, "over"));
    let (clearing, below) = (node(&mut ui, "clearing"), node(&mut ui, "below"));
    let owner = ui.host().semantics_owner();
    let hits_at = |x: f32| {
        let mut hits = Vec::new();
        owner.hit_test(Offset::new(x, 5.0), |info| {
            hits.push(info.semantics_id());
            false
        });
        hits
    };
    let on_siblings = hits_at(5.0);
    assert!(on_siblings.contains(&over.id()));
    assert!(!on_siblings.contains(&under.id()));
    let on_clearing = hits_at(55.0);
    assert!(on_clearing.contains(&clearing.id()));
    assert!(!on_clearing.contains(&below.id()));
}
