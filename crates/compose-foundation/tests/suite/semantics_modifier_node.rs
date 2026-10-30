//! A semantics modifier node's configuration kept per layout node (D1257): computed when a tree
//! or an action asks, kept until invalidated (`invalidate_semantics`, a chain change, an
//! observed read written), and its bounds' flags. Compose computes it at once when invalidated;
//! compose-rs when next asked, so the counts below are of the reads after invalidations.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::*;

/// What a test's semantics node shares with the test: its block, how often it ran, its handle.
struct Shared {
    apply: Box<dyn Fn(&mut SemanticsConfiguration)>,
    invocations: Cell<u32>,
    handle: Cell<Option<NodeHandle>>,
    merge: bool,
    important_for_bounds: bool,
}

/// Compose's `TestSemanticsModifier`: a semantics node running `apply`, counting its runs.
#[derive(Clone)]
struct TestSemantics(Rc<Shared>);

impl TestSemantics {
    fn new(apply: impl Fn(&mut SemanticsConfiguration) + 'static) -> TestSemantics {
        TestSemantics::with(true, false, apply)
    }

    fn with(
        important_for_bounds: bool,
        merge: bool,
        apply: impl Fn(&mut SemanticsConfiguration) + 'static,
    ) -> TestSemantics {
        TestSemantics(Rc::new(Shared {
            apply: Box::new(apply),
            invocations: Cell::new(0),
            handle: Cell::new(None),
            merge,
            important_for_bounds,
        }))
    }

    fn invocations(&self) -> u32 {
        self.0.invocations.get()
    }

    fn reset_counters(&self) {
        self.0.invocations.set(0);
    }

    fn invalidate_semantics(&self) {
        self.0.handle.get().expect("attached").invalidate_semantics();
    }

    fn modifier(&self) -> Modifier {
        Modifier.then_custom(self.clone())
    }
}

struct TestSemanticsNode(Rc<Shared>);

impl ModifierNodeElement for TestSemantics {
    type Node = TestSemanticsNode;
    fn create(&self) -> TestSemanticsNode {
        TestSemanticsNode(self.0.clone())
    }
    fn update(&self, node: &mut TestSemanticsNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ModifierNode for TestSemanticsNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.0.handle.set(Some(context.handle()));
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl SemanticsModifierNode for TestSemanticsNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        self.0.invocations.set(self.0.invocations.get() + 1);
        (self.0.apply)(config);
    }
    fn should_merge_descendant_semantics(&self) -> bool {
        self.0.merge
    }
    fn is_important_for_bounds(&self) -> bool {
        self.0.important_for_bounds
    }
}

/// A rule whose semantics trees don't check the kept configurations (checking computes them
/// again, which these tests count, and one test keeps a stale one on purpose).
fn unchecked(content: impl Fn() + 'static) -> ComposeTestRule {
    let ui = compose_test(content);
    compose_ui::platform::set_semantics_cache_check(false);
    ui
}

fn tagged(tag: &'static str) -> TestSemantics {
    TestSemantics::new(move |config| config.set_test_tag(tag))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, applySemantics_firstComposition
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, applySemantics_calledWhenSemanticsIsRead
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, applySemantics_calledWhenFetchSemanticsNodeIsCalled
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, applySemantics_multipleCallsToFetchSemanticsNodeDoesNotTriggerMultipleCalls
/// Computed once, at the first read (Compose's at composition), then kept however often it's
/// read.
#[test]
fn semantics_are_computed_once_and_kept() {
    let semantics = tagged("TestTag");
    let given = semantics.clone();
    let mut ui = unchecked(move || Box!(modifier = given.modifier()));
    assert_eq!(semantics.invocations(), 0, "nothing asked yet");
    ui.on_node_with_tag("TestTag").assert_exists();
    assert_eq!(semantics.invocations(), 1);
    semantics.reset_counters();
    ui.on_node_with_tag("TestTag").assert_exists();
    for _ in 0..3 {
        ui.on_node_with_tag("TestTag").fetch_semantics_snapshot();
    }
    assert_eq!(semantics.invocations(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, applySemantics_InResponseToInvalidateSemantics
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, applySemantics_calledAgainInResponseToInvalidateSemantics
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, applySemantics_calledInResponseToMultipleInvalidateSemantics
/// `invalidate_semantics` has them computed again at the next read, once however many times
/// it was asked (Compose computes them at each).
#[test]
fn invalidating_computes_again_at_the_next_read() {
    let semantics = tagged("TestTag");
    let given = semantics.clone();
    let mut ui = unchecked(move || Box!(modifier = given.modifier()));
    ui.on_node_with_tag("TestTag").assert_exists();
    semantics.reset_counters();
    semantics.invalidate_semantics();
    ui.wait_for_idle();
    assert_eq!(semantics.invocations(), 0, "not until read");
    ui.on_node_with_tag("TestTag").assert_exists();
    assert_eq!(semantics.invocations(), 1);
    for _ in 0..6 {
        semantics.invalidate_semantics();
        ui.wait_for_idle();
    }
    ui.on_node_with_tag("TestTag").assert_exists();
    assert_eq!(semantics.invocations(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, applySemantics_calledAutomaticallyInResponseToChangesToObservedReads
/// A state read in `apply_semantics` and written has them computed again.
#[test]
fn an_observed_read_written_computes_again() {
    let tag = mutable_state_of("tag1");
    let handle = tag.handle();
    let semantics = TestSemantics::new(move |config| config.set_test_tag(handle.get()));
    let given = semantics.clone();
    let mut ui = unchecked(move || Box!(modifier = given.modifier()));
    ui.on_node_with_tag("tag1").assert_exists();
    semantics.reset_counters();
    tag.set("tag2");
    ui.on_node_with_tag("tag2").assert_exists();
    assert_eq!(semantics.invocations(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, applySemantics_notCalledAutomaticallyInResponseToChangesToNonObservedReads
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, applySemantics_calledInResponseToChangesToNonObservedReads_whenInvalidated
/// A plain value changed isn't seen until the node invalidates its semantics.
#[test]
fn an_unobserved_change_needs_an_invalidation() {
    let tag = Rc::new(RefCell::new("tag1"));
    let read = tag.clone();
    let semantics = TestSemantics::new(move |config| config.set_test_tag(*read.borrow()));
    let given = semantics.clone();
    let mut ui = unchecked(move || Box!(modifier = given.modifier()));
    ui.on_node_with_tag("tag1").assert_exists();
    semantics.reset_counters();
    *tag.borrow_mut() = "tag2";
    ui.on_node_with_tag("tag2").assert_does_not_exist();
    assert_eq!(semantics.invocations(), 0);
    semantics.invalidate_semantics();
    ui.on_node_with_tag("tag2").assert_exists();
    assert_eq!(semantics.invocations(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, applySemantics_alongWithModifierRemoval
/// A chain change (a modifier before it removed) has them computed again after the update,
/// though the node doesn't auto-invalidate.
#[test]
fn a_chain_change_computes_again() {
    struct Enabled(bool);
    struct EnabledNode(bool);
    impl ModifierNodeElement for Enabled {
        type Node = EnabledNode;
        fn create(&self) -> EnabledNode {
            EnabledNode(self.0)
        }
        fn update(&self, node: &mut EnabledNode) {
            node.0 = self.0;
        }
        fn same(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl ModifierNode for EnabledNode {
        fn should_auto_invalidate(&self) -> bool {
            false
        }
        fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
            Some(self)
        }
    }
    impl SemanticsModifierNode for EnabledNode {
        fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
            if !self.0 {
                config.disabled();
            }
        }
    }
    let enabled = mutable_state_of(true);
    let handle = enabled.handle();
    let mut ui = unchecked(move || {
        let on = handle.get();
        let sized = if on { Modifier.size(10.dp()) } else { Modifier };
        Box!(modifier = sized.test_tag("tag").then_custom(Enabled(on)));
    });
    ui.on_node_with_tag("tag").assert_is_enabled();
    enabled.set(false);
    ui.on_node_with_tag("tag").assert_is_not_enabled();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, applySemantics_notCalledForDeactivatedNode
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, invalidateSemantics_onDeactivatedNodeDoesNotCrash
/// A lazy item kept for reuse isn't in the semantics: nothing computes its own, and
/// invalidating it does nothing.
#[test]
fn a_deactivated_node_computes_nothing() {
    let semantics: Vec<TestSemantics> =
        (0..2).map(|index| tagged(if index == 0 { "0" } else { "1" })).collect();
    let given = semantics.clone();
    let state = LazyListState::new(0, 0);
    let list = *state;
    let mut ui = unchecked(move || {
        let given = given.clone();
        LazyRow!(modifier = Modifier.size(10.dp()), state = list, |scope: LazyListScope| {
            let given = given.clone();
            scope.items(2, move |index| {
                Box!(modifier = Modifier.size(10.dp()).then(given[index].modifier()));
            });
        });
    });
    ui.wait_for_idle();
    semantics[0].reset_counters();
    state.request_scroll_to_item(1, 0);
    ui.wait_for_idle();
    ui.on_node_with_tag("0").assert_does_not_exist();
    assert_eq!(semantics[0].invocations(), 0);
    if let Some(handle) = semantics[0].0.handle.get() {
        handle.invalidate_semantics();
    }
    ui.on_node_with_tag("0").assert_does_not_exist();
    assert_eq!(semantics[0].invocations(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, invalidateSemantics_calledWithinApplySemantics_doesNotTriggerAnotherCallToApplySemantics
/// An invalidation asked while they're computed doesn't count.
#[test]
fn invalidating_while_computing_does_not_count() {
    let slot: Rc<RefCell<Option<TestSemantics>>> = Rc::default();
    let inside = slot.clone();
    let semantics = TestSemantics::new(move |config| {
        config.set_test_tag("tag");
        if let Some(semantics) = &*inside.borrow() {
            semantics.invalidate_semantics();
        }
    });
    *slot.borrow_mut() = Some(semantics.clone());
    let given = semantics.clone();
    let mut ui = unchecked(move || Box!(modifier = given.modifier()));
    ui.on_node_with_tag("tag").assert_exists();
    semantics.reset_counters();
    ui.wait_for_idle();
    ui.on_node_with_tag("tag").assert_exists();
    assert_eq!(semantics.invocations(), 0);
    slot.borrow_mut().take();
}

/// compose-test's rules check the kept configurations at each tree in debug builds (D1261): a
/// missed invalidation (a node changing what its semantics read without saying so) fails the
/// test, naming the node.
#[cfg(debug_assertions)]
#[test]
fn a_rule_finds_a_stale_configuration() {
    let tag = Rc::new(RefCell::new("tag1"));
    let read = tag.clone();
    let semantics = TestSemantics::new(move |config| config.set_test_tag(*read.borrow()));
    let given = semantics.clone();
    let mut ui = compose_test(move || Box!(modifier = given.modifier()));
    ui.on_node_with_tag("tag1").assert_exists();
    *tag.borrow_mut() = "tag2";
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ui.on_node_with_tag("tag2").assert_exists();
    }));
    let message = failed.expect_err("stale").downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(message.contains("are stale"), "{message}");
    compose_ui::platform::set_semantics_cache_check(false);
}

// ---- Actions ----------------------------------------------------------------------------------

/// Performing a node's action reads its own semantics, not the tree's: a sibling's, invalidated,
/// isn't computed again (D1257).
#[test]
fn performing_an_action_builds_no_tree() {
    let sibling = tagged("sibling");
    let given = sibling.clone();
    let clicks = Rc::new(Cell::new(0));
    let counted = clicks.clone();
    let mut ui = unchecked(move || {
        let (counted, given) = (counted.clone(), given.clone());
        Column!({
            let counted = counted.clone();
            Box!(
                modifier = Modifier
                    .clickable(move || counted.set(counted.get() + 1))
                    .size(10.dp())
                    .test_tag("button")
            );
            Box!(modifier = given.modifier().size(10.dp()));
        });
    });
    let button = ui.on_node_with_tag("button").node_id();
    sibling.invalidate_semantics();
    ui.wait_for_idle();
    sibling.reset_counters();
    assert!(ui.host().perform_semantics_action(button, &SemanticsActions::ON_CLICK));
    assert_eq!(clicks.get(), 1);
    assert_eq!(sibling.invocations(), 0, "the sibling's semantics weren't computed");
}

/// A clickable's click is built once per update, not per read: the same function after its
/// semantics are computed again (a focus change), a new one when the clickable is updated
/// (D1258).
#[test]
fn a_clickables_click_is_built_once_per_update() {
    let generation = mutable_state_of(0);
    let handle = generation.handle();
    let mut ui = unchecked(move || {
        let current = handle.get();
        Box!(
            modifier = Modifier
                .clickable(move || {
                    let _ = current;
                })
                .size(10.dp())
                .test_tag("button")
        );
    });
    let button = ui.on_node_with_tag("button").node_id();
    let first = ui.host().semantics_action(button, &SemanticsActions::ON_CLICK).unwrap();
    ui.on_node_with_tag("button").request_focus();
    ui.on_node_with_tag("button").assert_is_focused();
    let again = ui.host().semantics_action(button, &SemanticsActions::ON_CLICK).unwrap();
    assert!(Rc::ptr_eq(&first, &again), "the same click after a focus change");
    generation.set(1);
    ui.wait_for_idle();
    let updated = ui.host().semantics_action(button, &SemanticsActions::ON_CLICK).unwrap();
    assert!(!Rc::ptr_eq(&first, &updated), "a new click for the new callback");
}

// ---- Bounds -------------------------------------------------------------------------------------

/// `outer`, then a 10 dp size and 1 dp of padding, then `inner`, tagged: the node's semantic
/// bounds are the outer layer's (0, 0, 10 × 10) or the inner's (1, 1, 8 × 8).
#[track_caller]
fn bounds_follow(outer: TestSemantics, inner: Option<TestSemantics>, inner_layer: bool) {
    let mut ui = unchecked(move || {
        let mut modifier = outer.modifier().size(10.dp()).padding(1.dp());
        if let Some(inner) = &inner {
            modifier = modifier.then(inner.modifier());
        }
        Box!(modifier = modifier.test_tag("tag"));
    });
    let (offset, size) = if inner_layer { (1.dp(), 8.dp()) } else { (0.dp(), 10.dp()) };
    ui.on_node_with_tag("tag")
        .assert_position_in_root_is_equal_to(offset, offset)
        .assert_width_is_equal_to(size)
        .assert_height_is_equal_to(size)
        .assert_touch_width_is_equal_to(size)
        .assert_touch_height_is_equal_to(size);
}

fn plain(important: bool) -> TestSemantics {
    TestSemantics::with(important, false, |_| {})
}

fn merging(important: bool) -> TestSemantics {
    TestSemantics::with(important, true, |_| {})
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, isImportantForBounds_defaultValue_semanticsModifierInfluencesBounds
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, isImportantForBounds_notImportant_semanticsModifierDoesNotInfluenceBounds
#[test]
fn a_modifier_important_for_bounds_gives_its_bounds() {
    bounds_follow(plain(true), None, false);
    // The test tag's (inner) layer, the outer not being important.
    bounds_follow(plain(false), None, true);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, isImportantForBounds_innerShouldMergeDescendants_innerTakesPriority
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, isImportantForBounds_innerShouldMergeDescendantsNotImportant_outerTakesPriority
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, isImportantForBounds_outerShouldMergeDescendants_outerTakesPriority
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, isImportantForBounds_outerShouldMergeDescendantsNotImportant_innerTakesPriority
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, isImportantForBounds_multipleShouldMergeDescendantsModifiers_outerTakesPriority
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt, isImportantForBounds_multipleShouldMergeDescendantsModifiers_outerNotImportant_innerTakesPriority
/// The outermost merging modifier important for bounds gives them, else the outermost
/// important one.
#[test]
fn the_outermost_important_merging_modifier_gives_the_bounds() {
    bounds_follow(plain(true), Some(merging(true)), true);
    bounds_follow(plain(true), Some(merging(false)), false);
    bounds_follow(merging(true), Some(plain(true)), false);
    bounds_follow(merging(false), Some(plain(true)), true);
    bounds_follow(merging(true), Some(merging(true)), false);
    bounds_follow(merging(false), Some(merging(true)), true);
}
