//! `DelegatingNode`: `Delegates`, dispatch by kind, delegates' lifecycle
//! (docs/parity/design/modifier-nodes.md §3.7, step MN-5).

use crate::support;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;
use support::*;

thread_local! {
    /// The handles of the nodes attached, by label, as the test's `val`s.
    static HANDLES: RefCell<HashMap<&'static str, NodeHandle>> = RefCell::new(HashMap::new());
}

/// Forgets the last test's handles (a thread can run several tests).
fn fresh() {
    HANDLES.with(|handles| handles.borrow_mut().clear());
    take_log();
}

fn handle(label: &str) -> NodeHandle {
    HANDLES.with(|handles| *handles.borrow().get(label).expect("the node attached"))
}

/// As the test's `DrawMod`, `LayoutMod`, `SemanticsMod` and `DelegatingNode`s: what it implements
/// is chosen per instance, and it may delegate. Logs its lifecycle and what it's asked.
struct Mod {
    label: &'static str,
    draw: bool,
    layout: bool,
    semantics: bool,
    delegates: Delegates,
    /// What `wrapping` delegated to.
    delegated_to: Option<Delegate<Mod>>,
    /// As a layout node: how far it moves its content.
    inset: i32,
}

impl Mod {
    fn plain(label: &'static str) -> Mod {
        Mod {
            label,
            draw: false,
            layout: false,
            semantics: false,
            delegates: Delegates::new(),
            delegated_to: None,
            inset: 0,
        }
    }

    fn draw(label: &'static str) -> Mod {
        Mod { draw: true, ..Mod::plain(label) }
    }

    fn layout(label: &'static str) -> Mod {
        Mod { layout: true, ..Mod::plain(label) }
    }

    fn semantics(label: &'static str) -> Mod {
        Mod { semantics: true, ..Mod::plain(label) }
    }

    /// As `DelegatedWrapper`: delegating to `wrapped`.
    fn wrapping(label: &'static str, wrapped: Mod) -> Mod {
        let mut node = Mod::plain(label);
        node.delegated_to = Some(node.delegates.delegate(wrapped));
        node
    }

    fn with(mut self, delegate: Mod) -> Mod {
        self.delegates.delegate(delegate);
        self
    }
}

impl ModifierNode for Mod {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        if self.draw { Some(self) } else { None }
    }
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        if self.layout { Some(self) } else { None }
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        if self.semantics { Some(self) } else { None }
    }
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        assert!(context.is_attached(), "marked attached before its on_attach");
        let handle = context.handle();
        HANDLES.with(|handles| handles.borrow_mut().insert(self.label, handle));
        log(format!("attach {}", self.label));
    }
    fn on_detach(&mut self, context: &mut NodeContext<'_>) {
        assert!(context.is_attached(), "still attached in its on_detach");
        log(format!("detach {}", self.label));
    }
    fn on_reset(&mut self, _: &mut NodeContext<'_>) {
        log(format!("reset {}", self.label));
    }
    fn on_release(&mut self, _: &mut ReleaseContext<'_>) {
        log(format!("release {}", self.label));
    }
}

impl DrawModifierNode for Mod {
    fn draw(&mut self, _: &mut DrawScope<'_>) {
        log(format!("draw {}", self.label));
    }
}

impl LayoutModifierNode for Mod {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        log(format!("measure {}", self.label));
        let placeable = scope.measure(content, constraints);
        scope.place(&placeable, self.inset, self.inset);
        scope.layout(placeable.width, placeable.height)
    }
}

impl SemanticsModifierNode for Mod {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.set_test_tag(self.label);
    }
}

/// An element making a node with `make`, never updating it.
#[derive(Clone)]
struct Element(Rc<dyn Fn() -> Mod>);

impl ModifierNodeElement for Element {
    type Node = Mod;
    fn create(&self) -> Mod {
        (self.0)()
    }
    fn update(&self, _: &mut Mod) {}
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

fn element(make: impl Fn() -> Mod + 'static) -> Element {
    Element(Rc::new(make))
}

/// As the test's `layout(a, b, c)`: a layout node with a chain of `elements`.
fn layout(elements: Vec<Element>) -> UiHost {
    fresh();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        let mut modifier = Modifier;
        for element in &elements {
            modifier = modifier.then_custom(element.clone());
        }
        Leaf(modifier);
    });
    host.frame();
    host
}

/// Runs `f` with the attached node `label`: the test's `node.delegateUnprotected(…)`.
fn on<R>(label: &str, f: impl FnOnce(&mut Mod) -> R) -> R {
    handle(label).with_node(|node: &mut Mod, _| f(node)).expect("the tree is free between frames")
}

/// Which of (draw, layout, semantics) the kinds have.
fn three(kinds: NodeKinds) -> (bool, bool, bool) {
    (kinds.any(NodeKinds::DRAW), kinds.any(NodeKinds::LAYOUT), kinds.any(NodeKinds::SEMANTICS))
}

/// (draw, layout, semantics) of `kindSet`.
fn kinds(label: &str) -> (bool, bool, bool) {
    three(handle(label).kinds_for_test().expect("the node").0)
}

/// (draw, layout, semantics) of `aggregateChildKindSet`: `chain.has` for the chain's head.
fn aggregate(label: &str) -> (bool, bool, bool) {
    three(handle(label).kinds_for_test().expect("the node").1)
}

const NONE: (bool, bool, bool) = (false, false, false);
const DRAW: (bool, bool, bool) = (true, false, false);
const LAYOUT: (bool, bool, bool) = (false, true, false);
const SEMANTICS: (bool, bool, bool) = (false, false, true);

/// `node`'s dispatch for `kind`, as the test's `assertDispatchOrder`.
fn dispatch_order(node: &mut Mod, kind: NodeKinds) -> Vec<&'static str> {
    let mut order = Vec::new();
    dispatch_for_kind_of(node, kind, &mut |node| {
        order.push(node.downcast_ref::<Mod>().expect("a test node").label);
    });
    order
}

/// The library's `asKind(Nodes.Draw)`.
fn as_draw_kind(node: &mut Mod) -> Option<&'static str> {
    let found = first_of_kind_of(node, NodeKinds::DRAW)?;
    Some(found.downcast_ref::<Mod>().expect("a test node").label)
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testKindSetIncludesDelegates
#[test]
fn a_node_has_the_kinds_of_its_delegates() {
    let _host = layout(vec![
        element(|| Mod::wrapping("wrapper", Mod::draw("wrapped"))),
        element(|| Mod::plain("tail")),
    ]);
    assert_eq!(kinds("wrapper"), DRAW);
    assert_eq!(kinds("tail"), NONE);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testKindSetUpdatesAfter
#[test]
fn delegating_updates_the_kinds_and_the_aggregates_up_the_chain() {
    let _host = layout(vec![
        element(|| Mod::draw("a")),
        element(|| Mod::wrapping("b", Mod::layout("b.wrapped"))),
        element(|| Mod::plain("c")),
    ]);
    assert_eq!(aggregate("a"), (true, true, false));
    assert_eq!(kinds("a"), DRAW);
    assert_eq!(kinds("b"), LAYOUT);
    assert_eq!(aggregate("b"), LAYOUT);
    assert_eq!(kinds("c"), NONE);
    assert_eq!(aggregate("c"), NONE);

    on("c", |c| {
        c.delegates.delegate(Mod::semantics("c.semantics"));
    });

    assert_eq!(aggregate("a"), (true, true, true));
    assert_eq!(kinds("a"), DRAW);
    assert_eq!(kinds("b"), LAYOUT);
    assert_eq!(aggregate("b"), (false, true, true));
    assert_eq!(kinds("c"), SEMANTICS);
    assert_eq!(aggregate("c"), SEMANTICS);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testAsKindReturnsDelegate
#[test]
fn as_kind_finds_the_delegate() {
    let mut node = Mod::wrapping("wrapper", Mod::draw("wrapped"));
    assert_eq!(as_draw_kind(&mut node), Some("wrapped"));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testNestedDelegatesHaveNodePointersCorrectlyUpdated
// Compose's `node` is the chain's node a delegate works for; here, a delegate's context has the
// place of its owner: its layout node and layer.
#[test]
fn nested_delegates_work_where_their_owner_does() {
    let _host = layout(vec![element(|| {
        Mod::plain("a").with(Mod::plain("b").with(Mod::draw("c"))).with(Mod::draw("d"))
    })]);
    let place = handle("a").with_node(|_: &mut Mod, context| context.layout_node()).unwrap();
    for label in ["b", "c", "d"] {
        let found = handle(label).with_node(|_: &mut Mod, context| context.layout_node()).unwrap();
        assert_eq!(found, place, "{label}");
        assert_eq!(handle(label).layer_for_test(), handle("a").layer_for_test(), "{label}");
    }
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testAsKindReturnsNestedDelegate
#[test]
fn as_kind_finds_a_nested_delegate() {
    let mut node = Mod::wrapping("outer", Mod::wrapping("inner", Mod::draw("wrapped")));
    assert_eq!(as_draw_kind(&mut node), Some("wrapped"));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testAsKindReturnsSelf
#[test]
fn as_kind_finds_the_node_itself_first() {
    let mut node = Mod::draw("self").with(Mod::draw("wrapped"));
    assert_eq!(as_draw_kind(&mut node), Some("self"));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testAsKindMultipleDelegatesReturnsLast
#[test]
fn as_kind_with_several_delegates_finds_the_last() {
    let mut node = Mod::plain("node").with(Mod::draw("first")).with(Mod::draw("second"));
    assert_eq!(as_draw_kind(&mut node), Some("second"));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testDispatchForMultipleDelegatesSameKind
#[test]
fn dispatch_goes_to_each_delegate_of_the_kind_in_delegation_order() {
    let mut node = Mod::plain("node")
        .with(Mod::wrapping("first", Mod::draw("first.wrapped")))
        .with(Mod::draw("second"));
    assert_eq!(dispatch_order(&mut node, NodeKinds::DRAW), ["first.wrapped", "second"]);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testDispatchForSelfOnlyDispatchesToSelf
#[test]
fn dispatch_to_a_node_of_the_kind_goes_to_it_alone() {
    let mut node = Mod::draw("self").with(Mod::draw("wrapped"));
    assert_eq!(dispatch_order(&mut node, NodeKinds::DRAW), ["self"]);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testDispatchNestedSelfStops
#[test]
fn dispatch_stops_at_a_delegate_of_the_kind() {
    let mut node = Mod::plain("node")
        .with(Mod::draw("first"))
        .with(Mod::draw("second"))
        .with(Mod::draw("third").with(Mod::draw("third.first")).with(Mod::draw("third.second")));
    assert_eq!(dispatch_order(&mut node, NodeKinds::DRAW), ["first", "second", "third"]);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testHeadToTailNoDelegates
#[test]
fn nodes_without_delegates_draw_head_to_tail() {
    let _host = layout(vec![element(|| Mod::draw("a")), element(|| Mod::draw("b"))]);
    assert_eq!(take_log().iter().filter(|line| line.starts_with("draw")).count(), 2);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testHeadToTailWithDelegate
#[test]
fn a_delegate_draws_in_its_owner_s_place_in_the_chain() {
    let _host = layout(vec![
        element(|| Mod::wrapping("a", Mod::draw("a.wrapped"))),
        element(|| Mod::draw("b")),
    ]);
    let draws: Vec<String> =
        take_log().into_iter().filter(|line| line.starts_with("draw")).collect();
    assert_eq!(draws, ["draw a.wrapped", "draw b"]);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testUndelegate
#[test]
fn undelegating_detaches_the_delegate_and_takes_its_kinds_away() {
    let _host = layout(vec![element(|| Mod::plain("node"))]);
    assert_eq!(kinds("node"), NONE);
    let draw = on("node", |node| node.delegates.delegate(Mod::draw("draw")));
    assert_eq!(kinds("node"), DRAW);
    assert_eq!(aggregate("node"), DRAW);
    let draw_handle = handle("draw");
    assert!(draw_handle.is_attached());

    on("node", |node| node.delegates.undelegate(draw));

    assert!(!draw_handle.is_attached());
    assert!(handle("node").is_attached());
    assert_eq!(kinds("node"), NONE);
    assert_eq!(aggregate("node"), NONE);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testUndelegateWithMultipleDelegates
#[test]
fn undelegating_one_of_several_keeps_the_others_kinds() {
    let _host = layout(vec![element(|| Mod::plain("node"))]);
    let (semantics, draw2) = on("node", |node| {
        node.delegates.delegate(Mod::draw("draw"));
        node.delegates.delegate(Mod::layout("layout"));
        let semantics = node.delegates.delegate(Mod::semantics("semantics"));
        (semantics, node.delegates.delegate(Mod::draw("draw2")))
    });
    assert_eq!(kinds("node"), (true, true, true));
    assert_eq!(on("node", as_draw_kind), Some("draw2"));

    on("node", |node| node.delegates.undelegate(semantics));
    assert_eq!(kinds("node"), (true, true, false));
    assert_eq!(aggregate("node"), (true, true, false));
    assert_eq!(on("node", as_draw_kind), Some("draw2"));

    on("node", |node| node.delegates.undelegate(draw2));
    assert_eq!(kinds("node"), (true, true, false));
    assert_eq!(on("node", as_draw_kind), Some("draw"));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testDelegateUndelegateInChain
#[test]
fn delegating_along_a_chain_updates_the_aggregates_before_it_only() {
    let _host = layout(vec![
        element(|| Mod::plain("a")),
        element(|| Mod::plain("b")),
        element(|| Mod::plain("c")),
    ]);
    assert_eq!(aggregate("a"), NONE);

    let draw = on("c", |c| c.delegates.delegate(Mod::draw("draw")));
    assert_eq!((kinds("a"), kinds("b"), kinds("c")), (NONE, NONE, DRAW));
    assert_eq!((aggregate("a"), aggregate("b"), aggregate("c")), (DRAW, DRAW, DRAW));

    let semantics = on("b", |b| b.delegates.delegate(Mod::semantics("semantics")));
    assert_eq!((kinds("a"), kinds("b"), kinds("c")), (NONE, SEMANTICS, DRAW));
    let both = (true, false, true);
    assert_eq!((aggregate("a"), aggregate("b"), aggregate("c")), (both, both, DRAW));

    let layout_delegate = on("a", |a| a.delegates.delegate(Mod::layout("layout")));
    assert_eq!((kinds("a"), kinds("b"), kinds("c")), (LAYOUT, SEMANTICS, DRAW));
    assert_eq!((aggregate("a"), aggregate("b"), aggregate("c")), ((true, true, true), both, DRAW));

    on("c", |c| c.delegates.undelegate(draw));
    assert_eq!((kinds("a"), kinds("b"), kinds("c")), (LAYOUT, SEMANTICS, NONE));
    assert_eq!(
        (aggregate("a"), aggregate("b"), aggregate("c")),
        ((false, true, true), SEMANTICS, NONE)
    );

    on("b", |b| b.delegates.undelegate(semantics));
    assert_eq!((kinds("a"), kinds("b"), kinds("c")), (LAYOUT, NONE, NONE));
    assert_eq!((aggregate("a"), aggregate("b"), aggregate("c")), (LAYOUT, NONE, NONE));

    on("a", |a| a.delegates.undelegate(layout_delegate));
    assert_eq!((kinds("a"), kinds("b"), kinds("c")), (NONE, NONE, NONE));
    assert_eq!((aggregate("a"), aggregate("b"), aggregate("c")), (NONE, NONE, NONE));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testDelegateUndelegateInNode
#[test]
fn delegating_and_undelegating_on_one_node_adds_and_takes_away_each_kind() {
    let _host = layout(vec![element(|| Mod::plain("node"))]);
    let draw = on("node", |node| node.delegates.delegate(Mod::draw("draw")));
    assert_eq!(kinds("node"), DRAW);
    let semantics = on("node", |node| node.delegates.delegate(Mod::semantics("semantics")));
    assert_eq!(kinds("node"), (true, false, true));
    let layout_delegate = on("node", |node| node.delegates.delegate(Mod::layout("layout")));
    assert_eq!(kinds("node"), (true, true, true));
    assert_eq!(aggregate("node"), (true, true, true));

    on("node", |node| node.delegates.undelegate(draw));
    assert_eq!(kinds("node"), (false, true, true));
    on("node", |node| node.delegates.undelegate(semantics));
    assert_eq!(kinds("node"), LAYOUT);
    on("node", |node| node.delegates.undelegate(layout_delegate));
    assert_eq!(kinds("node"), NONE);
    assert_eq!(aggregate("node"), NONE);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testDelegateUndelegateNested
#[test]
fn nested_delegates_kinds_reach_the_owner_and_leave_it() {
    let _host = layout(vec![element(|| Mod::plain("node"))]);
    let draw = on("node", |node| {
        node.delegates.delegate(Mod::wrapping("draw", Mod::draw("draw.wrapped")))
    });
    assert_eq!(kinds("node"), DRAW);
    let semantics = on("draw", |draw| {
        draw.delegates.delegate(Mod::wrapping("sem", Mod::semantics("sem.wrapped")))
    });
    assert_eq!(kinds("node"), (true, false, true));
    assert_eq!(aggregate("node"), (true, false, true));
    let layout_delegate = on("sem", |sem| sem.delegates.delegate(Mod::layout("layout")));
    assert_eq!(kinds("node"), (true, true, true));

    on("sem", |sem| sem.delegates.undelegate(layout_delegate));
    assert_eq!(kinds("node"), (true, false, true));
    on("draw", |draw| draw.delegates.undelegate(semantics));
    assert_eq!(kinds("node"), DRAW);
    on("node", |node| node.delegates.undelegate(draw));
    assert_eq!(kinds("node"), NONE);
    assert_eq!(aggregate("node"), NONE);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testUndelegateForNestedDelegate
#[test]
fn undelegating_from_a_delegate_leaves_its_sibling_be() {
    let _host = layout(vec![element(|| Mod::plain("a"))]);
    on("a", |a| {
        a.delegates.delegate(Mod::plain("b"));
        a.delegates.delegate(Mod::plain("c"));
    });
    assert_eq!(kinds("a"), NONE);
    let draw = on("c", |c| c.delegates.delegate(Mod::draw("draw")));
    assert_eq!(kinds("a"), DRAW);
    assert_eq!(kinds("b"), NONE);
    assert_eq!(kinds("c"), DRAW);

    on("c", |c| c.delegates.undelegate(draw));
    assert_eq!(kinds("a"), NONE);
    assert_eq!(kinds("b"), NONE);
    assert_eq!(kinds("c"), NONE);
}

/// `content` shown by a layout node whose element is replaced (an update) when `version` changes,
/// and the frame's stats: an updated node is auto-invalidated by its and its delegates' kinds.
struct Updated {
    version: u32,
}

struct UpdatedNode {
    delegates: Delegates,
}

impl ModifierNodeElement for Updated {
    type Node = UpdatedNode;
    fn create(&self) -> UpdatedNode {
        let mut delegates = Delegates::new();
        delegates.delegate(Mod::draw("draw"));
        delegates.delegate(Mod::layout("layout"));
        delegates.delegate(Mod::semantics("semantics"));
        UpdatedNode { delegates }
    }
    fn update(&self, _: &mut UpdatedNode) {}
    fn same(&self, other: &Self) -> bool {
        self.version == other.version
    }
}

impl ModifierNode for UpdatedNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testInvalidateInsertedNode
// Compose calls its auto-invalidation directly; here a node's update asks it, by the same kinds.
#[test]
fn a_node_s_auto_invalidation_goes_by_its_delegates_kinds() {
    fresh();
    let version = mutable_state_of(0u32);
    let v = version.handle();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || Leaf(Modifier.then_custom(Updated { version: v.get() })));
    host.frame();
    take_log();
    version.set(1);
    let stats = host.frame();
    assert!(stats.measured);
    assert!(stats.drew);
    assert!(stats.semantics_changed);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testNestedNodeInvalidation
#[test]
fn a_nested_delegate_s_kinds_alone_are_auto_invalidated() {
    struct Nested(u32);
    struct NestedNode(Delegates);
    impl ModifierNodeElement for Nested {
        type Node = NestedNode;
        fn create(&self) -> NestedNode {
            let mut delegates = Delegates::new();
            delegates.delegate(Mod::wrapping("outer", Mod::wrapping("inner", Mod::draw("draw"))));
            NestedNode(delegates)
        }
        fn update(&self, _: &mut NestedNode) {}
        fn same(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl ModifierNode for NestedNode {
        fn delegates(&mut self) -> Option<&mut Delegates> {
            Some(&mut self.0)
        }
    }
    fresh();
    let version = mutable_state_of(0u32);
    let v = version.handle();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || Leaf(Modifier.then_custom(Nested(v.get()))));
    host.frame();
    version.set(1);
    let stats = host.frame();
    assert!(stats.drew);
    assert!(!stats.measured);
    assert!(!stats.semantics_changed);
}

/// A draw delegate deciding its own auto-invalidation, under an owner deciding its own.
struct Opinionated {
    version: u32,
    owner_invalidates: bool,
    delegate_invalidates: bool,
}

struct OpinionatedOwner {
    invalidates: bool,
    delegates: Delegates,
}

struct OpinionatedDelegate(bool);

impl ModifierNodeElement for Opinionated {
    type Node = OpinionatedOwner;
    fn create(&self) -> OpinionatedOwner {
        let mut delegates = Delegates::new();
        delegates.delegate(OpinionatedDelegate(self.delegate_invalidates));
        OpinionatedOwner { invalidates: self.owner_invalidates, delegates }
    }
    fn update(&self, _: &mut OpinionatedOwner) {}
    fn same(&self, other: &Self) -> bool {
        self.version == other.version
    }
}

impl ModifierNode for OpinionatedOwner {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn should_auto_invalidate(&self) -> bool {
        self.invalidates
    }
}

impl LayoutModifierNode for OpinionatedOwner {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

impl ModifierNode for OpinionatedDelegate {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn should_auto_invalidate(&self) -> bool {
        self.0
    }
}

impl DrawModifierNode for OpinionatedDelegate {}

fn updated_frame(owner_invalidates: bool, delegate_invalidates: bool) -> FrameStats {
    fresh();
    let version = mutable_state_of(0u32);
    let v = version.handle();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        Leaf(Modifier.then_custom(Opinionated {
            version: v.get(),
            owner_invalidates,
            delegate_invalidates,
        }))
    });
    host.frame();
    version.set(1);
    host.frame()
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/NodeKind.kt, autoInvalidateNodeSelf
#[test]
fn the_node_and_each_delegate_decide_their_own_auto_invalidation() {
    let stats = updated_frame(true, true);
    assert!(stats.measured);
    assert!(stats.drew);
    // The owner says no, the delegate yes: drawn, not measured.
    let stats = updated_frame(false, true);
    assert!(!stats.measured);
    assert!(stats.drew);
    // The owner yes, the delegate no: measured (and so drawn, D1164).
    let stats = updated_frame(true, false);
    assert!(stats.measured);
    // Both no.
    let stats = updated_frame(false, false);
    assert!(!stats.measured);
    assert!(!stats.drew);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testDelegateUndelegateCausesInvalidationsForDelegateKindsOnly
#[test]
fn delegating_invalidates_by_the_delegate_s_kinds_only() {
    let mut host = layout(vec![element(|| Mod::wrapping("node", Mod::semantics("semantics")))]);
    host.frame();

    let draw = on("node", |node| node.delegates.delegate(Mod::draw("draw")));
    assert!(host.needs_frame());
    let stats = host.frame();
    assert!(stats.drew);
    assert!(!stats.measured);
    assert!(!stats.semantics_changed);

    on("node", |node| node.delegates.undelegate(draw));
    assert!(host.needs_frame());
    let stats = host.frame();
    assert!(stats.drew);
    assert!(!stats.measured);
    assert!(!stats.semantics_changed);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testDelegatingToLayoutNodeUpdatesCoordinators
// Compose's coordinators are compose-rs's layers.
#[test]
fn delegating_to_a_layout_node_makes_a_layer_and_undelegating_takes_it_away() {
    let mut host = layout(vec![
        element(|| Mod::draw("a")),
        element(|| Mod::plain("b")),
        element(|| Mod::layout("c")),
    ]);
    assert_eq!(handle("a").layer_for_test(), Some((0, 2)));
    assert_eq!(handle("b").layer_for_test(), Some((0, 2)));
    assert_eq!(handle("c").layer_for_test(), Some((0, 2)));

    let delegate = on("b", |b| b.delegates.delegate(Mod::layout("lm")));

    assert_eq!(handle("a").layer_for_test(), Some((0, 3)));
    assert_eq!(handle("b").layer_for_test(), Some((0, 3)));
    assert_eq!(handle("c").layer_for_test(), Some((1, 3)));
    take_log();
    let stats = host.frame();
    assert!(stats.measured, "the new layer measures");
    let measures: Vec<String> =
        take_log().into_iter().filter(|line| line.starts_with("measure")).collect();
    assert_eq!(measures, ["measure lm", "measure c"]);

    on("b", |b| b.delegates.undelegate(delegate));

    assert_eq!(handle("a").layer_for_test(), Some((0, 2)));
    assert_eq!(handle("b").layer_for_test(), Some((0, 2)));
    assert_eq!(handle("c").layer_for_test(), Some((0, 2)));
    take_log();
    host.frame();
    let measures: Vec<String> =
        take_log().into_iter().filter(|line| line.starts_with("measure")).collect();
    assert_eq!(measures, ["measure c"]);
}

/// A host showing `make`'s node while `active`: deactivated and reactivated as kept for reuse,
/// Compose's `chain.runDetachLifecycle()` and `markAsAttached()`.
fn reusable_layout(active: MutableState<bool>, make: impl Fn() -> Mod + 'static) -> UiHost {
    fresh();
    let element = element(make);
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        let element = element.clone();
        ReusableContentHost(active.get(), move || {
            let element = element.clone();
            ReusableContent(0, move || Leaf(Modifier.then_custom(element.clone())));
        });
    });
    host.frame();
    host
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testDelegateAttachDetach
#[test]
fn delegates_attach_and_detach_with_their_owner() {
    let active = mutable_state_of(true);
    let mut host =
        reusable_layout(active.handle(), || Mod::wrapping("a", Mod::wrapping("b", Mod::draw("c"))));
    let (a, b, c) = (handle("a"), handle("b"), handle("c"));
    assert!(a.is_attached() && b.is_attached() && c.is_attached());

    active.set(false);
    host.frame();
    assert!(!a.is_attached() && !b.is_attached() && !c.is_attached());

    active.set(true);
    host.frame();
    assert!(a.is_attached() && b.is_attached() && c.is_attached());

    // Undelegated, innermost first, they're no longer attached, and the owner stays.
    let delegate_c = on("b", |b| b.delegated_to.expect("the wrapped node"));
    let delegate_b = on("a", |a| a.delegated_to.expect("the wrapped node"));
    on("b", |b| b.delegates.undelegate(delegate_c));
    on("a", |a| a.delegates.undelegate(delegate_b));
    assert!(a.is_attached());
    assert!(!b.is_attached());
    assert!(!c.is_attached());
}

/// Delegates `b` in its `on_attach`, undelegates it in its `on_detach` if `undelegates`.
struct DelegatesInAttach {
    undelegates: bool,
    delegates: Delegates,
    b: Option<Delegate<Mod>>,
}

struct DelegatesInAttachElement {
    undelegates: bool,
}

impl ModifierNodeElement for DelegatesInAttachElement {
    type Node = DelegatesInAttach;
    fn create(&self) -> DelegatesInAttach {
        DelegatesInAttach { undelegates: self.undelegates, delegates: Delegates::new(), b: None }
    }
    fn update(&self, _: &mut DelegatesInAttach) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for DelegatesInAttach {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn on_attach(&mut self, _: &mut NodeContext<'_>) {
        if self.b.is_none() {
            self.b = Some(self.delegates.delegate(Mod::draw("b")));
        }
    }
    fn on_detach(&mut self, _: &mut NodeContext<'_>) {
        if self.undelegates {
            let b = self.b.take().expect("delegated in on_attach");
            self.delegates.undelegate(b);
        }
    }
}

fn reusable_delegating_in_attach(active: MutableState<bool>, undelegates: bool) -> UiHost {
    fresh();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        ReusableContentHost(active.get(), move || {
            ReusableContent(0, move || {
                Leaf(Modifier.then_custom(DelegatesInAttachElement { undelegates }))
            });
        });
    });
    host.frame();
    host
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testDelegateInAttachUndelegateInDetach
// compose-rs's nodes are owned by what holds them: the delegate delegated again is a new one.
#[test]
fn delegating_in_attach_and_undelegating_in_detach() {
    let active = mutable_state_of(true);
    let mut host = reusable_delegating_in_attach(active.handle(), true);
    let first = handle("b");
    assert!(first.is_attached());

    active.set(false);
    host.frame();
    assert!(!first.is_attached());
    let lifecycle: Vec<String> =
        take_log().into_iter().filter(|line| !line.starts_with("draw")).collect();
    assert_eq!(lifecycle, ["attach b", "reset b", "detach b", "release b"]);

    active.set(true);
    host.frame();
    let second = handle("b");
    assert_ne!(first, second);
    assert!(second.is_attached());
    assert!(!first.is_attached());
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testDelegateInAttach
#[test]
fn delegating_in_attach_keeps_the_delegate_through_a_detach() {
    let active = mutable_state_of(true);
    let mut host = reusable_delegating_in_attach(active.handle(), false);
    let b = handle("b");
    assert!(b.is_attached());

    active.set(false);
    host.frame();
    assert!(!b.is_attached());

    active.set(true);
    host.frame();
    assert_eq!(handle("b"), b);
    assert!(b.is_attached());
}

// ---- The lifecycle's orders -------------------------------------------------------------------

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, runAttachLifecycle
#[test]
fn delegates_attach_before_their_owner_the_newest_first() {
    let _host = layout(vec![element(|| {
        Mod::plain("owner")
            .with(Mod::wrapping("first", Mod::plain("nested")))
            .with(Mod::plain("second"))
    })]);
    assert_eq!(take_log(), ["attach second", "attach nested", "attach first", "attach owner"]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, runDetachLifecycle
#[test]
fn delegates_reset_and_detach_after_their_owner_the_newest_first() {
    let active = mutable_state_of(true);
    let mut host = reusable_layout(active.handle(), || {
        Mod::plain("owner")
            .with(Mod::wrapping("first", Mod::plain("nested")))
            .with(Mod::plain("second"))
    });
    take_log();
    active.set(false);
    host.frame();
    assert_eq!(
        take_log(),
        [
            "reset owner",
            "reset second",
            "reset first",
            "reset nested",
            "detach owner",
            "detach second",
            "detach first",
            "detach nested",
        ]
    );
}

/// A delegate's node is released with its owner, and undelegated at once.
#[test]
fn delegates_are_released_with_their_owner() {
    fresh();
    let shown = mutable_state_of(true);
    let s = shown.handle();
    let element = element(|| Mod::wrapping("owner", Mod::wrapping("first", Mod::plain("nested"))));
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        if s.get() {
            Leaf(Modifier.then_custom(element.clone()));
        }
    });
    host.frame();
    let delegate = handle("nested");
    take_log();
    shown.set(false);
    host.frame();
    let log = take_log();
    assert!(!delegate.is_attached());
    for label in ["owner", "first", "nested"] {
        assert!(log.contains(&format!("release {label}")), "{log:?}");
    }
    assert!(delegate.with_node(|_: &mut Mod, _| ()).is_none(), "the delegate is gone");
}

/// A delegate removed by a chain change goes through detach and release too.
#[test]
fn delegates_of_a_node_removed_from_its_chain_detach_and_release() {
    fresh();
    let with_node = mutable_state_of(true);
    let w = with_node.handle();
    let element = element(|| Mod::wrapping("owner", Mod::plain("delegate")));
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        let modifier = if w.get() { Modifier.then_custom(element.clone()) } else { Modifier };
        Leaf(modifier);
    });
    host.frame();
    let delegate = handle("delegate");
    take_log();
    with_node.set(false);
    host.frame();
    assert!(!delegate.is_attached());
    assert_eq!(
        take_log(),
        ["detach owner", "detach delegate", "release delegate", "release owner"]
    );
}

/// Compose's error, for two layout delegates under an owner that isn't one.
#[test]
#[should_panic(expected = "delegating to several layout nodes")]
fn two_layout_delegates_panic_under_an_owner_that_is_not_a_layout_node() {
    let _host = layout(vec![element(|| {
        Mod::plain("owner").with(Mod::layout("first")).with(Mod::layout("second"))
    })]);
}

/// An owner that is a layout node itself may have several, and measures itself.
#[test]
fn a_layout_node_may_delegate_to_layout_nodes() {
    let _host = layout(vec![element(|| {
        Mod::layout("owner").with(Mod::layout("first")).with(Mod::layout("second"))
    })]);
    let measures: Vec<String> =
        take_log().into_iter().filter(|line| line.starts_with("measure")).collect();
    assert_eq!(measures, ["measure owner"]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, undelegate
#[test]
#[should_panic(expected = "could not find the delegate")]
fn undelegating_twice_panics() {
    let mut node = Mod::plain("node");
    let delegate = node.delegates.delegate(Mod::plain("delegate"));
    node.delegates.undelegate(delegate);
    node.delegates.undelegate(delegate);
}

// ---- Dispatch to delegates through the tree ---------------------------------------------------

/// Takes pointer input, keys and focus, counting what it hears.
struct Interactive {
    label: &'static str,
    observer: bool,
}

impl ModifierNode for Interactive {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn is_pointer_observer(&self) -> bool {
        self.observer
    }
}

impl PointerInputModifierNode for Interactive {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _: IntSize,
        _: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Main && event.kind == PointerEventType::Press {
            log(format!("press {}", self.label));
        }
    }
}

impl KeyInputModifierNode for Interactive {
    fn on_key_event(&mut self, event: &KeyEvent, _: &mut EventContext<'_>) -> bool {
        if event.kind == KeyEventType::KeyDown {
            log(format!("key {}", self.label));
        }
        false
    }
}

impl FocusTargetModifierNode for Interactive {
    fn on_focus_changed(
        &mut self,
        _previous: FocusState,
        current: FocusState,
        _context: &mut EventContext<'_>,
    ) {
        let focused = current.is_focused();
        log(format!("focus {} {focused}", self.label));
    }
}

/// Delegates to two `Interactive`s, and can stop delegating to the first.
struct TwoInteractive {
    delegates: Delegates,
    first: Delegate<Interactive>,
}

struct TwoInteractiveElement;

impl ModifierNodeElement for TwoInteractiveElement {
    type Node = TwoInteractive;
    fn create(&self) -> TwoInteractive {
        let mut delegates = Delegates::new();
        let first = delegates.delegate(Interactive { label: "first", observer: false });
        delegates.delegate(Interactive { label: "second", observer: false });
        TwoInteractive { delegates, first }
    }
    fn update(&self, _: &mut TwoInteractive) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for TwoInteractive {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        let handle = context.handle();
        HANDLES.with(|handles| handles.borrow_mut().insert("two", handle));
    }
}

fn two_interactive() -> UiHost {
    fresh();
    let mut host = UiHost::headless(200, 100);
    host.set_content(|| Leaf(Modifier.size(50.dp()).then_custom(TwoInteractiveElement)));
    host.frame();
    host
}

#[test]
fn pointer_events_go_to_each_delegate_in_delegation_order() {
    let mut host = two_interactive();
    host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    assert_eq!(take_log(), ["press first", "press second"]);
    host.send_pointer(PointerEvent::release(Offset::new(10.0, 10.0)));

    // Undelegated, it hears no more, and the other goes on.
    handle("two")
        .with_node(|node: &mut TwoInteractive, _| {
            let first = node.first;
            node.delegates.undelegate(first);
        })
        .expect("the tree is free");
    host.frame();
    host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    assert_eq!(take_log(), ["press second"]);
}

#[test]
fn a_gesture_in_progress_goes_on_for_the_delegates_that_remain() {
    let mut host = two_interactive();
    host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    take_log();
    handle("two")
        .with_node(|node: &mut TwoInteractive, _| {
            let first = node.first;
            node.delegates.undelegate(first);
        })
        .expect("the tree is free");
    // The path holds an undelegated node: it's passed over.
    host.send_pointer(PointerEvent::release(Offset::new(10.0, 10.0)));
    host.frame();
    assert_eq!(take_log(), Vec::<String>::new());
}

#[test]
fn the_focus_goes_to_the_first_focus_target_delegate_and_keys_to_each_key_delegate() {
    let mut host = two_interactive();
    let leaf = host.with_tree(|tree| tree.children_of(tree.root_id()).next().expect("the leaf"));
    host.with_tree(|tree| tree.request_focus(leaf));
    // As `visitChildren`'s `dispatchForKind`: the targets of a node in the order delegated.
    assert_eq!(take_log(), ["focus first true"]);
    host.send_key(KeyEvent::down(Key::Enter));
    assert_eq!(take_log(), ["key first", "key second"]);
}

#[test]
fn a_delegate_s_semantics_are_its_owner_s() {
    let host = layout(vec![element(|| Mod::wrapping("owner", Mod::semantics("tagged")))]);
    let tree = host.semantics_tree(false);
    fn has_tag(node: &SemanticsSnapshot, tag: &str) -> bool {
        node.config.get(&SemanticsProperties::TEST_TAG).is_some_and(|found| &**found == tag)
            || node.children.iter().any(|child| has_tag(child, tag))
    }
    assert!(has_tag(&tree, "tagged"));
}

/// A delegate that delegates in its pointer handler, as Compose's `LazyDelegationExample`.
struct LazyDelegating {
    delegates: Delegates,
}

impl ModifierNode for LazyDelegating {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for LazyDelegating {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _: IntSize,
        _: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Main && event.kind == PointerEventType::Press {
            self.delegates.delegate(Mod::draw("lazy"));
        }
    }
}

struct LazyDelegatingElement;

impl ModifierNodeElement for LazyDelegatingElement {
    type Node = LazyDelegating;
    fn create(&self) -> LazyDelegating {
        LazyDelegating { delegates: Delegates::new() }
    }
    fn update(&self, _: &mut LazyDelegating) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

// Compose: compose/ui/ui/samples/src/main/java/androidx/compose/ui/samples/ModifierSamples.kt, LazyDelegationExample
#[test]
fn delegating_from_a_pointer_handler_attaches_after_the_dispatch() {
    fresh();
    let mut host = UiHost::headless(200, 100);
    host.set_content(|| Leaf(Modifier.size(50.dp()).then_custom(LazyDelegatingElement)));
    host.frame();
    host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    assert_eq!(take_log(), ["attach lazy"]);
    assert!(host.needs_frame(), "a new draw delegate draws");
    host.frame();
    assert_eq!(take_log(), ["draw lazy"]);
}

/// Delegates or undelegates by its element's `delegating`, as Compose's
/// `ConditionalDelegationExample`.
struct Conditional {
    delegating: bool,
}

struct ConditionalNode {
    delegates: Delegates,
    delegate: Option<Delegate<Mod>>,
}

impl ConditionalNode {
    fn set(&mut self, delegating: bool) {
        match (delegating, self.delegate) {
            (true, None) => self.delegate = Some(self.delegates.delegate(Mod::draw("conditional"))),
            (false, Some(delegate)) => {
                self.delegates.undelegate(delegate);
                self.delegate = None;
            }
            _ => {}
        }
    }
}

impl ModifierNodeElement for Conditional {
    type Node = ConditionalNode;
    fn create(&self) -> ConditionalNode {
        let mut node = ConditionalNode { delegates: Delegates::new(), delegate: None };
        node.set(self.delegating);
        node
    }
    fn update(&self, node: &mut ConditionalNode) {
        node.set(self.delegating);
    }
    fn same(&self, other: &Self) -> bool {
        self.delegating == other.delegating
    }
}

impl ModifierNode for ConditionalNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
}

// Compose: compose/ui/ui/samples/src/main/java/androidx/compose/ui/samples/ModifierSamples.kt, ConditionalDelegationExample
#[test]
fn delegating_from_an_element_s_update_goes_through_when_the_pass_ends() {
    fresh();
    let delegating = mutable_state_of(false);
    let d = delegating.handle();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || Leaf(Modifier.then_custom(Conditional { delegating: d.get() })));
    host.frame();
    assert_eq!(take_log(), Vec::<String>::new());

    delegating.set(true);
    let stats = host.frame();
    assert!(stats.drew);
    assert_eq!(take_log(), ["attach conditional", "draw conditional"]);
    let delegate = handle("conditional");

    delegating.set(false);
    let stats = host.frame();
    assert!(stats.drew);
    assert_eq!(take_log(), ["detach conditional", "release conditional"]);
    assert!(!delegate.is_attached());
}

/// A draw delegate wrapping its content.
struct Wrapping(&'static str);

impl ModifierNode for Wrapping {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for Wrapping {
    fn draws_content(&self) -> bool {
        true
    }
    fn draw_with_content(&mut self, scope: &mut ContentDrawScope<'_, '_>) {
        log(format!("before {}", self.0));
        scope.draw_content();
        log(format!("after {}", self.0));
    }
}

struct TwoWrapping;
struct TwoWrappingNode(Delegates);

impl ModifierNodeElement for TwoWrapping {
    type Node = TwoWrappingNode;
    fn create(&self) -> TwoWrappingNode {
        let mut delegates = Delegates::new();
        delegates.delegate(Wrapping("first"));
        delegates.delegate(Wrapping("second"));
        TwoWrappingNode(delegates)
    }
    fn update(&self, _: &mut TwoWrappingNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for TwoWrappingNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.0)
    }
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/LayoutNodeDrawScope.kt, drawDirect
#[test]
fn each_draw_delegate_draws_what_s_inside_it_as_its_content() {
    fresh();
    let mut host = UiHost::headless(200, 100);
    host.set_content(|| {
        Leaf(Modifier.then_custom(TwoWrapping).then_custom(element(|| Mod::draw("inside"))));
    });
    host.frame();
    assert_eq!(
        take_log().into_iter().filter(|line| !line.starts_with("attach")).collect::<Vec<_>>(),
        [
            "before first",
            "draw inside",
            "after first",
            "before second",
            "draw inside",
            "after second"
        ]
    );
}

/// A delegate's coroutine scope ends with it, and its observed reads tell it.
struct Watching {
    runs: Rc<Cell<u32>>,
}

impl ModifierNode for Watching {
    fn as_observer(&mut self) -> Option<&mut dyn ObserverModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        let handle = context.handle();
        HANDLES.with(|handles| handles.borrow_mut().insert("watching", handle));
    }
}

impl ObserverModifierNode for Watching {
    fn on_observed_reads_changed(&mut self, _: &mut NodeContext<'_>) {
        self.runs.set(self.runs.get() + 1);
    }
}

struct WatchingElement(Rc<Cell<u32>>);
struct WatchingOwner {
    delegates: Delegates,
    watching: Delegate<Watching>,
}

impl ModifierNodeElement for WatchingElement {
    type Node = WatchingOwner;
    fn create(&self) -> WatchingOwner {
        let mut delegates = Delegates::new();
        let watching = delegates.delegate(Watching { runs: self.0.clone() });
        WatchingOwner { delegates, watching }
    }
    fn update(&self, _: &mut WatchingOwner) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for WatchingOwner {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        let handle = context.handle();
        HANDLES.with(|handles| handles.borrow_mut().insert("owner", handle));
    }
}

#[test]
fn a_delegate_observes_reads_and_is_told_with_its_own_context() {
    fresh();
    let runs = Rc::new(Cell::new(0));
    let value = mutable_state_of(0);
    let r = runs.clone();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || Leaf(Modifier.then_custom(WatchingElement(r.clone()))));
    host.frame();
    let read = value.handle();
    handle("watching")
        .with_node(|_: &mut Watching, context| context.observe_reads(|| read.get()))
        .expect("the tree is free");
    value.set(1);
    host.frame();
    assert_eq!(runs.get(), 1);
}

#[test]
fn undelegating_cancels_a_delegate_s_scope_and_forgets_what_it_observes() {
    fresh();
    let runs = Rc::new(Cell::new(0));
    let value = mutable_state_of(0);
    let r = runs.clone();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || Leaf(Modifier.then_custom(WatchingElement(r.clone()))));
    host.frame();
    let read = value.handle();
    let scope = handle("watching")
        .with_node(|_: &mut Watching, context| {
            context.observe_reads(|| read.get());
            context.coroutine_scope()
        })
        .expect("the tree is free");
    let job = scope.launch(std::future::pending::<()>());
    assert!(job.is_active());
    handle("owner")
        .with_node(|owner: &mut WatchingOwner, _| {
            let watching = owner.watching;
            owner.delegates.undelegate(watching);
        })
        .expect("the tree is free");
    assert!(!job.is_active());
    value.set(1);
    host.frame();
    assert_eq!(runs.get(), 0);
}

// ---- Delegated cases of other Compose tests ---------------------------------------------------

struct PositionAware(Rc<RefCell<Vec<Offset>>>);

impl ModifierNode for PositionAware {
    fn as_global_position_aware(&mut self) -> Option<&mut dyn GlobalPositionAwareModifierNode> {
        Some(self)
    }
}

impl GlobalPositionAwareModifierNode for PositionAware {
    fn on_globally_positioned(&mut self, coordinates: LayoutCoordinates, _: &mut NodeContext<'_>) {
        self.0.borrow_mut().push(coordinates.position_in_root());
    }
}

struct PositionAwareOwner(Delegates);

struct PositionAwareElement(Rc<RefCell<Vec<Offset>>>, u32);

impl ModifierNodeElement for PositionAwareElement {
    type Node = PositionAwareOwner;
    fn create(&self) -> PositionAwareOwner {
        let mut delegates = Delegates::new();
        for _ in 0..self.1 {
            delegates.delegate(PositionAware(self.0.clone()));
        }
        PositionAwareOwner(delegates)
    }
    fn update(&self, _: &mut PositionAwareOwner) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for PositionAwareOwner {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.0)
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnGloballyPositionedTest.kt, delegatedGloballyPositionedNode
#[test]
fn a_position_aware_delegate_is_told_where_it_is() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        let padding = PaddingValues::new(10.dp(), 12.dp(), 0.dp(), 0.dp());
        Leaf(Modifier.padding(padding).then_custom(PositionAwareElement(s.clone(), 1)));
    });
    host.frame();
    assert_eq!(&*seen.borrow(), &[Offset::new(10.0, 12.0)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnGloballyPositionedTest.kt, delegatedMultipleGloballyPositionedNodes
#[test]
fn several_position_aware_delegates_are_each_told() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || Leaf(Modifier.then_custom(PositionAwareElement(s.clone(), 2))));
    host.frame();
    assert_eq!(seen.borrow().len(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/CustomLayoutAndMeasureTest.kt, instancesKeepDelegates
#[test]
fn a_layout_modifier_s_content_stays_the_same_measurable_across_measurements() {
    struct Capture(Rc<RefCell<Option<Measurable>>>);
    struct CaptureNode(Rc<RefCell<Option<Measurable>>>);
    impl ModifierNodeElement for Capture {
        type Node = CaptureNode;
        fn create(&self) -> CaptureNode {
            CaptureNode(self.0.clone())
        }
        fn update(&self, _: &mut CaptureNode) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    impl ModifierNode for CaptureNode {
        fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
            Some(self)
        }
    }
    impl LayoutModifierNode for CaptureNode {
        fn measure(
            &mut self,
            scope: &mut MeasureScope<'_>,
            content: Measurable,
            constraints: Constraints,
        ) -> MeasureResult {
            *self.0.borrow_mut() = Some(content);
            let placeable = scope.measure(content, constraints);
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        }
    }
    let captured = Rc::new(RefCell::new(None));
    let size = mutable_state_of(30);
    let color = mutable_state_of(Color::from_argb(0xFFFF_0000));
    let (c, s, k) = (captured.clone(), size.handle(), color.handle());
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        Leaf(Modifier.then_custom(Capture(c.clone())).background(k.get()).size(s.get().dp()));
    });
    host.frame();
    let first = captured.borrow_mut().take().expect("measured");
    size.set(40);
    color.set(Color::from_argb(0xFF00_00FF));
    host.frame();
    let second = captured.borrow_mut().take().expect("measured again");
    assert_eq!(first, second);
}

// ---- A review's scenarios ---------------------------------------------------------------------

/// Delegates to `Mod`s, and undelegates them in its `on_detach`, as Compose's
/// `AbstractClickableNode.onDetach`.
struct UndelegatesInDetach {
    delegates: Delegates,
    delegate: Option<Delegate<Mod>>,
}

struct UndelegatesInDetachElement;

impl ModifierNodeElement for UndelegatesInDetachElement {
    type Node = UndelegatesInDetach;
    fn create(&self) -> UndelegatesInDetach {
        let mut delegates = Delegates::new();
        let delegate = Some(delegates.delegate(Mod::plain("delegate")));
        UndelegatesInDetach { delegates, delegate }
    }
    fn update(&self, _: &mut UndelegatesInDetach) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for UndelegatesInDetach {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn on_detach(&mut self, _: &mut NodeContext<'_>) {
        if let Some(delegate) = self.delegate.take() {
            self.delegates.undelegate(delegate);
        }
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, AbstractClickableNode.onDetach
#[test]
fn undelegating_in_on_detach_of_a_node_leaving_its_chain_detaches_the_delegate() {
    fresh();
    let with_node = mutable_state_of(true);
    let w = with_node.handle();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        let modifier =
            if w.get() { Modifier.then_custom(UndelegatesInDetachElement) } else { Modifier };
        Leaf(modifier);
    });
    host.frame();
    let delegate = handle("delegate");
    let scope = delegate.with_node(|_: &mut Mod, context| context.coroutine_scope()).unwrap();
    let job = scope.launch(std::future::pending::<()>());
    take_log();
    with_node.set(false);
    host.frame();
    assert_eq!(take_log(), ["detach delegate", "release delegate"]);
    assert!(!delegate.is_attached());
    assert!(!job.is_active(), "its scope is cancelled");
}

/// Undelegates by its element's `delegating`, as `Conditional`, with a `Mod` to tell.
struct UndelegatedInUpdate {
    keep: bool,
}

struct UndelegatedInUpdateNode {
    delegates: Delegates,
    delegate: Option<Delegate<Mod>>,
}

impl ModifierNodeElement for UndelegatedInUpdate {
    type Node = UndelegatedInUpdateNode;
    fn create(&self) -> UndelegatedInUpdateNode {
        let mut delegates = Delegates::new();
        let delegate = Some(delegates.delegate(Mod::plain("delegate")));
        UndelegatedInUpdateNode { delegates, delegate }
    }
    fn update(&self, node: &mut UndelegatedInUpdateNode) {
        if !self.keep
            && let Some(delegate) = node.delegate.take()
        {
            node.delegates.undelegate(delegate);
        }
    }
    fn same(&self, other: &Self) -> bool {
        self.keep == other.keep
    }
}

impl ModifierNode for UndelegatedInUpdateNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
}

/// An element's update, then the node leaving its chain, in one pass (two chain changes applied
/// by the tree directly: a composition makes one per pass, but a paused one's resumes add up).
/// Compose detaches what's undelegated in `update` at once.
#[test]
fn a_delegate_undelegated_in_an_update_detaches_when_its_owner_leaves_in_the_same_pass() {
    fresh();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || Leaf(Modifier.then_custom(UndelegatedInUpdate { keep: true })));
    host.frame();
    let delegate = handle("delegate");
    take_log();
    let leaf = host.with_tree(|tree| tree.children_of(tree.root_id()).next().expect("the leaf"));
    host.with_tree(|tree| {
        // One pass's two chain changes: an update undelegating, then the node removed.
        tree.set_modifier(leaf, Modifier.then_custom(UndelegatedInUpdate { keep: false }));
        tree.set_modifier(leaf, Modifier);
        compose_runtime::platform::Applier::on_end_changes(tree);
    });
    assert_eq!(take_log(), ["detach delegate", "release delegate"]);
    assert!(!delegate.is_attached());
}

/// Delegates to a layout node while its element says so; takes pointer input through a sibling.
struct LayoutDelegateFlag(bool);

struct LayoutDelegateFlagNode {
    delegates: Delegates,
    delegate: Option<Delegate<Mod>>,
}

impl LayoutDelegateFlagNode {
    fn set(&mut self, on: bool) {
        match (on, self.delegate) {
            (true, None) => {
                let padding = Mod { inset: 10, ..Mod::layout("padding") };
                self.delegate = Some(self.delegates.delegate(padding));
            }
            (false, Some(delegate)) => {
                self.delegates.undelegate(delegate);
                self.delegate = None;
            }
            _ => {}
        }
    }
}

impl ModifierNodeElement for LayoutDelegateFlag {
    type Node = LayoutDelegateFlagNode;
    fn create(&self) -> LayoutDelegateFlagNode {
        let mut node = LayoutDelegateFlagNode { delegates: Delegates::new(), delegate: None };
        node.set(self.0);
        node
    }
    fn update(&self, node: &mut LayoutDelegateFlagNode) {
        node.set(self.0);
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl ModifierNode for LayoutDelegateFlagNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
}

/// Logs each pointer event it hears, kind and pass (the main one).
struct PointerLog;
struct PointerLogNode;

impl ModifierNodeElement for PointerLog {
    type Node = PointerLogNode;
    fn create(&self) -> PointerLogNode {
        PointerLogNode
    }
    fn update(&self, _: &mut PointerLogNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for PointerLogNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for PointerLogNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _: IntSize,
        _: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Main {
            log(format!("{:?}", event.kind));
            log(format!("position {} {}", event.position.x, event.position.y));
        }
    }
}

fn layout_delegate_host(on: &OwnedState<bool>) -> UiHost {
    fresh();
    let o = on.handle();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        // The pointer node in the last layer, which a dropped layer takes away.
        Leaf(
            Modifier.size(50.dp()).then_custom(LayoutDelegateFlag(o.get())).then_custom(PointerLog),
        );
    });
    host.frame();
    host
}

/// The pointer events the log has: what the node heard, not the lifecycle around it.
fn pointer_log() -> Vec<String> {
    take_log()
        .into_iter()
        .filter(|line| {
            ["Press", "Release", "Exit"].contains(&&**line) || line.starts_with("position")
        })
        .skip_while(|line| line.starts_with("position"))
        .collect()
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, undelegate (syncCoordinators)
#[test]
fn a_layout_delegate_dropped_in_an_update_during_a_gesture_keeps_the_gesture() {
    let on = mutable_state_of(true);
    let mut host = layout_delegate_host(&on);
    host.send_pointer(PointerEvent::press(Offset::new(20.0, 20.0)));
    assert_eq!(pointer_log(), ["Press", "position 10 10"]);
    on.set(false);
    host.frame();
    host.send_pointer(PointerEvent::release(Offset::new(20.0, 20.0)));
    // Its layer now has no inset before it.
    assert_eq!(pointer_log(), ["Release", "position 20 20"]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, delegate (syncCoordinators)
#[test]
fn a_layout_delegate_gained_in_an_update_during_a_gesture_keeps_the_gesture() {
    let on = mutable_state_of(false);
    let mut host = layout_delegate_host(&on);
    host.send_pointer(PointerEvent::press(Offset::new(20.0, 20.0)));
    assert_eq!(pointer_log(), ["Press", "position 20 20"]);
    on.set(true);
    host.frame();
    host.send_pointer(PointerEvent::release(Offset::new(20.0, 20.0)));
    // Its layer has the inset before it now.
    assert_eq!(pointer_log(), ["Release", "position 10 10"]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, undelegate (syncCoordinators)
#[test]
fn a_layout_delegate_dropped_in_an_update_keeps_a_resting_hover() {
    let on = mutable_state_of(true);
    let mut host = layout_delegate_host(&on);
    let mut moved = PointerEvent::moved(Offset::new(20.0, 20.0), false);
    moved.pointer_type = PointerType::Mouse;
    host.send_pointer(moved);
    take_log();
    on.set(false);
    host.frame();
    take_log();
    // The mouse leaves: the node it rested on hears it, in its new layer.
    host.send_pointer_left();
    let log = pointer_log();
    assert_eq!(log[0], "Exit");
}

/// Delegates when it gets the focus, as Compose's clickable's `onFocusChange` makes its
/// indication; and when its gesture is cancelled, and when it's positioned.
struct DelegatesOnEvents {
    delegates: Delegates,
    label: &'static str,
}

impl DelegatesOnEvents {
    fn new(label: &'static str) -> DelegatesOnEvents {
        DelegatesOnEvents { delegates: Delegates::new(), label }
    }
    fn add(&mut self) {
        self.delegates.delegate(Mod::draw(self.label));
    }
}

impl ModifierNode for DelegatesOnEvents {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_global_position_aware(&mut self) -> Option<&mut dyn GlobalPositionAwareModifierNode> {
        Some(self)
    }
}

impl FocusTargetModifierNode for DelegatesOnEvents {
    fn on_focus_changed(
        &mut self,
        _previous: FocusState,
        current: FocusState,
        _context: &mut EventContext<'_>,
    ) {
        let focused = current.is_focused();
        if focused && self.label == "focus" {
            self.add();
        }
    }
    fn on_focus_tap(&mut self, _: Offset, _: &mut EventContext<'_>) {
        if self.label == "tap" {
            self.add();
        }
    }
}

impl PointerInputModifierNode for DelegatesOnEvents {
    fn on_pointer_event(
        &mut self,
        _: &mut PointerEvent,
        _: PointerEventPass,
        _: IntSize,
        _: &mut EventContext<'_>,
    ) {
    }
    fn on_cancel_pointer_input(&mut self) {
        if self.label == "cancel" {
            self.add();
        }
    }
}

impl GlobalPositionAwareModifierNode for DelegatesOnEvents {
    fn on_globally_positioned(&mut self, _: LayoutCoordinates, _: &mut NodeContext<'_>) {
        if self.label == "positioned" && self.delegates.nodes().next().is_none() {
            self.add();
        }
    }
}

struct DelegatesOnEventsElement(&'static str);

impl ModifierNodeElement for DelegatesOnEventsElement {
    type Node = DelegatesOnEvents;
    fn create(&self) -> DelegatesOnEvents {
        DelegatesOnEvents::new(self.0)
    }
    fn update(&self, _: &mut DelegatesOnEvents) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

fn delegates_on_events(label: &'static str) -> (UiHost, NodeId) {
    fresh();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        Leaf(Modifier.size(50.dp()).then_custom(DelegatesOnEventsElement(label)))
    });
    host.frame();
    let leaf = host.with_tree(|tree| tree.children_of(tree.root_id()).next().expect("the leaf"));
    (host, leaf)
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, onFocusChange
#[test]
fn delegating_from_a_focus_callback_attaches_the_delegate() {
    let (host, leaf) = delegates_on_events("focus");
    assert_eq!(take_log(), Vec::<String>::new());
    host.with_tree(|tree| tree.request_focus(leaf));
    assert_eq!(take_log(), ["attach focus"]);
}

#[test]
fn delegating_when_a_gesture_is_cancelled_attaches_the_delegate() {
    let (mut host, _) = delegates_on_events("cancel");
    host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    host.with_tree(|tree| tree.cancel_pointers());
    assert_eq!(take_log(), ["attach cancel"]);
}

#[test]
fn delegating_when_positioned_attaches_the_delegate() {
    let (_host, _) = delegates_on_events("positioned");
    assert!(take_log().contains(&"attach positioned".to_string()));
}

/// Finds an owner through `find_nearest_ancestor`, then delegates on it.
#[test]
fn delegating_on_a_node_a_traversal_found_attaches_the_delegate() {
    static KEY: TraverseKey = TraverseKey::new("owner");
    struct Owner(Delegates);
    struct OwnerElement;
    impl ModifierNodeElement for OwnerElement {
        type Node = Owner;
        fn create(&self) -> Owner {
            Owner(Delegates::new())
        }
        fn update(&self, _: &mut Owner) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    impl ModifierNode for Owner {
        fn delegates(&mut self) -> Option<&mut Delegates> {
            Some(&mut self.0)
        }
        fn as_traversable(&mut self) -> Option<&mut dyn TraversableNode> {
            Some(self)
        }
    }
    impl TraversableNode for Owner {
        fn traverse_key(&self) -> &'static TraverseKey {
            &KEY
        }
    }
    fresh();
    let mut host = UiHost::headless(200, 100);
    host.set_content(|| {
        Leaf(Modifier.then_custom(OwnerElement).then_custom(element(|| Mod::plain("below"))));
    });
    host.frame();
    take_log();
    handle("below")
        .with_node(|_: &mut Mod, context| {
            let owner = context.find_nearest_ancestor_with_key(&KEY).expect("the owner");
            let owner = owner.downcast_mut::<Owner>().expect("of its type");
            owner.0.delegate(Mod::draw("found"));
        })
        .expect("the tree is free");
    assert_eq!(take_log(), ["attach found"]);
    assert!(handle("found").is_attached());
}

// ---- `delegate_attached`, as Compose's `delegate()` in a callback ----------------------------

struct DelegatesInItsAttach {
    delegates: Delegates,
    delegate_kind: NodeKinds,
}

struct DelegatesInItsAttachElement;

impl ModifierNodeElement for DelegatesInItsAttachElement {
    type Node = DelegatesInItsAttach;
    fn create(&self) -> DelegatesInItsAttach {
        DelegatesInItsAttach { delegates: Delegates::new(), delegate_kind: NodeKinds::NONE }
    }
    fn update(&self, _: &mut DelegatesInItsAttach) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for DelegatesInItsAttach {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        log("begin");
        self.delegates.delegate_attached(Mod::draw("d"), context);
        // Its kinds are the owner's already, and it's attached.
        self.delegate_kind = NodeKinds::DRAW;
        log("end");
    }
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, delegate
#[test]
fn delegating_attached_attaches_inside_the_call() {
    fresh();
    let mut host = UiHost::headless(200, 100);
    host.set_content(|| Leaf(Modifier.then_custom(DelegatesInItsAttachElement)));
    host.frame();
    let lifecycle: Vec<String> =
        take_log().into_iter().filter(|line| !line.starts_with("draw")).collect();
    assert_eq!(lifecycle, ["begin", "attach d", "end"]);
    assert!(handle("d").is_attached());
}

/// Reads its owner's kinds from inside its `on_attach`, through a handle made there.
struct ReadsKindsInAttach(Rc<Cell<bool>>);

impl ModifierNode for ReadsKindsInAttach {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.0.set(context.chain_kinds_for_test().any(NodeKinds::DRAW));
    }
}

impl DrawModifierNode for ReadsKindsInAttach {}

struct KindsFirstElement(Rc<Cell<bool>>);
struct KindsFirstNode(Delegates, Rc<Cell<bool>>);

impl ModifierNodeElement for KindsFirstElement {
    type Node = KindsFirstNode;
    fn create(&self) -> KindsFirstNode {
        KindsFirstNode(Delegates::new(), self.0.clone())
    }
    fn update(&self, _: &mut KindsFirstNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for KindsFirstNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.0)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.0.delegate_attached(ReadsKindsInAttach(self.1.clone()), context);
    }
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, delegate (updateNodeKindSet)
#[test]
fn delegating_attached_gives_the_owner_its_kinds_before_the_on_attach() {
    fresh();
    let seen = Rc::new(Cell::new(false));
    let s = seen.clone();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || Leaf(Modifier.then_custom(KindsFirstElement(s.clone()))));
    host.frame();
    assert!(seen.get());
}

// ---- Delegating in `on_detach` ----------------------------------------------------------------

struct DelegatesInDetach(Delegates);
struct DelegatesInDetachElement;

impl ModifierNodeElement for DelegatesInDetachElement {
    type Node = DelegatesInDetach;
    fn create(&self) -> DelegatesInDetach {
        DelegatesInDetach(Delegates::new())
    }
    fn update(&self, _: &mut DelegatesInDetach) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for DelegatesInDetach {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.0)
    }
    fn on_detach(&mut self, _: &mut NodeContext<'_>) {
        self.0.delegate(Mod::plain("late"));
    }
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, runDetachLifecycle
#[test]
fn a_delegate_attached_in_an_on_detach_hears_its_on_detach() {
    fresh();
    let with_node = mutable_state_of(true);
    let w = with_node.handle();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        Leaf(if w.get() { Modifier.then_custom(DelegatesInDetachElement) } else { Modifier });
    });
    host.frame();
    take_log();
    with_node.set(false);
    host.frame();
    assert_eq!(take_log(), ["attach late", "detach late", "release late"]);
}

#[test]
fn a_delegate_attached_in_a_kept_node_s_on_detach_hears_its_on_detach() {
    fresh();
    let active = mutable_state_of(true);
    let a = active.handle();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        ReusableContentHost(a.get(), move || {
            ReusableContent(0, move || Leaf(Modifier.then_custom(DelegatesInDetachElement)));
        });
    });
    host.frame();
    take_log();
    active.set(false);
    host.frame();
    assert_eq!(take_log(), ["attach late", "detach late"]);
}

// ---- compose-rs's extras, on delegates --------------------------------------------------------

/// A node that's only a delegating one, over `delegate`.
struct Over(Delegates);

impl ModifierNode for Over {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.0)
    }
}

/// An element of a node delegating to what `make` makes.
struct OverElement<N>(Rc<dyn Fn() -> N>);

impl<N: ModifierNode> ModifierNodeElement for OverElement<N> {
    type Node = Over;
    fn create(&self) -> Over {
        let mut delegates = Delegates::new();
        delegates.delegate((self.0)());
        Over(delegates)
    }
    fn update(&self, _: &mut Over) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

fn over<N: ModifierNode>(make: impl Fn() -> N + 'static) -> Modifier {
    Modifier.then_custom(OverElement(Rc::new(make)))
}

struct Boundary;

impl ModifierNode for Boundary {
    fn is_repaint_boundary(&self) -> bool {
        true
    }
}

#[test]
fn a_delegate_may_be_a_repaint_boundary() {
    let groups = |modifier: fn() -> Modifier| {
        let mut host = UiHost::headless(200, 100);
        host.set_content(move || Leaf(modifier().size(10.dp())));
        host.frame();
        host.with_tree(|tree| tree.display_list().groups.len())
    };
    assert_eq!(groups(|| over(|| Boundary)), groups(|| Modifier) + 1);
}

struct BeyondBounds(Rc<Cell<u32>>);

impl ModifierNode for BeyondBounds {
    fn is_beyond_bounds_layout(&self) -> bool {
        true
    }
    fn lay_out_beyond_bounds(&mut self, _: FocusDirection, _: LayoutDirection) -> bool {
        self.0.set(self.0.get() + 1);
        false
    }
}

#[test]
fn a_delegate_may_lay_out_beyond_bounds_for_a_focus_search() {
    let asked = Rc::new(Cell::new(0));
    let a = asked.clone();
    let requester = FocusRequester::new();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        let a = a.clone();
        // A group's search asks what's around it, as a scrolling container's.
        Stack(
            over(move || BeyondBounds(a.clone())).size(50.dp()).focus_group(),
            callback(move || {
                Leaf(Modifier.focus_requester(requester).focusable().size(10.dp()));
            }),
        );
    });
    host.frame();
    requester.request_focus();
    host.frame();
    host.with_tree(|tree| tree.move_focus_in(FocusDirection::Down));
    assert!(asked.get() > 0, "the search asked it");
}

struct Clips;

impl ModifierNode for Clips {
    fn hit_clip(&self) -> Option<Shape> {
        Some(RECTANGLE_SHAPE)
    }
}

/// What's outside a delegate's clip isn't hit, and its coordinates are clipped by it.
#[test]
fn a_delegate_may_clip_hits_and_bounds() {
    let clicks = Rc::new(Cell::new(0));
    let bounds = Rc::new(Cell::new(None));
    let (c, b) = (clicks.clone(), bounds.clone());
    let policy = measure_policy(|scope, measurables, constraints| {
        for &measurable in measurables {
            let placeable = scope.measure(measurable, constraints.loosen());
            scope.place(&placeable, 20, 0);
        }
        scope.layout(30, 30)
    });
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        let (c, b) = (c.clone(), b.clone());
        Layout(
            over(|| Clips),
            policy.clone(),
            callback(move || {
                let (c, b) = (c.clone(), b.clone());
                Leaf(
                    Modifier
                        .clickable(move || c.set(c.get() + 1))
                        .on_globally_positioned(move |coordinates: LayoutCoordinates| {
                            b.set(Some(coordinates.bounds_in_root()))
                        })
                        .size(20.dp()),
                );
            }),
        );
    });
    host.frame();
    click(&mut host, 25.0, 5.0);
    assert_eq!(clicks.get(), 1, "inside");
    click(&mut host, 35.0, 5.0);
    assert_eq!(clicks.get(), 1, "outside the delegate's clip");
    let bounds = bounds.get().expect("told");
    assert_eq!(bounds.right, 30.0, "clipped by the delegate");
}

struct Scrolls(Rc<Cell<u32>>);

impl ModifierNode for Scrolls {
    fn bring_into_view(&mut self, rect: Rect, _: IntSize) -> Option<Rect> {
        self.0.set(self.0.get() + 1);
        Some(rect)
    }
}

#[test]
fn a_delegate_may_scroll_something_into_view() {
    let asked = Rc::new(Cell::new(0));
    let a = asked.clone();
    let leaf = Rc::new(Cell::new(None));
    let l = leaf.clone();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        let (a, l) = (a.clone(), l.clone());
        Stack(
            over(move || Scrolls(a.clone())).size(50.dp()),
            callback(move || {
                Leaf(Modifier.then_custom(KnowsItsNodeElement(l.clone())).size(10.dp()));
            }),
        );
    });
    host.frame();
    let node = leaf.get().expect("attached");
    host.with_tree(|tree| tree.bring_into_view(node));
    assert_eq!(asked.get(), 1);
}

struct KnowsItsNode(Rc<Cell<Option<NodeId>>>);

impl ModifierNode for KnowsItsNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.0.set(Some(context.layout_node()));
    }
}

struct KnowsItsNodeElement(Rc<Cell<Option<NodeId>>>);

impl ModifierNodeElement for KnowsItsNodeElement {
    type Node = KnowsItsNode;
    fn create(&self) -> KnowsItsNode {
        KnowsItsNode(self.0.clone())
    }
    fn update(&self, _: &mut KnowsItsNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

// ---- An update's context, and each consumer told ------------------------------------------------

struct AsksContext(Delegates, Rc<RefCell<Vec<&'static str>>>);

impl ModifierNode for AsksContext {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.0)
    }
    fn on_update_with_context(&mut self, context: &mut NodeContext<'_>) {
        assert!(context.is_attached());
        self.1.borrow_mut().push("with context");
    }
    fn on_attach(&mut self, _: &mut NodeContext<'_>) {
        self.1.borrow_mut().push("attach");
    }
}

struct AsksContextElement(i32, Rc<RefCell<Vec<&'static str>>>);

impl ModifierNodeElement for AsksContextElement {
    type Node = AsksContext;
    fn create(&self) -> AsksContext {
        AsksContext(Delegates::new(), self.1.clone())
    }
    fn update(&self, node: &mut AsksContext) {
        node.0.ask_for_context();
        self.1.borrow_mut().push("update");
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// An `update` asking for a context gets it when its pass's lifecycle runs.
#[test]
fn an_update_may_ask_for_its_context() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let version = mutable_state_of(0);
    let (v, l) = (version.handle(), log.clone());
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        Leaf(Modifier.then_custom(AsksContextElement(v.get(), l.clone())));
    });
    host.frame();
    assert_eq!(*log.borrow(), ["attach"]);
    version.set(1);
    host.frame();
    assert_eq!(*log.borrow(), ["attach", "update", "with context"]);
}

/// Asked for again from there, it's dropped: nothing to spin on.
#[test]
fn asking_for_a_context_from_it_is_dropped() {
    struct AsksAgain(Delegates, Rc<Cell<u32>>);
    impl ModifierNode for AsksAgain {
        fn delegates(&mut self) -> Option<&mut Delegates> {
            Some(&mut self.0)
        }
        fn on_update_with_context(&mut self, _: &mut NodeContext<'_>) {
            self.1.set(self.1.get() + 1);
            self.0.ask_for_context();
        }
    }
    struct AsksAgainElement(i32, Rc<Cell<u32>>);
    impl ModifierNodeElement for AsksAgainElement {
        type Node = AsksAgain;
        fn create(&self) -> AsksAgain {
            AsksAgain(Delegates::new(), self.1.clone())
        }
        fn update(&self, node: &mut AsksAgain) {
            node.0.ask_for_context();
        }
        fn same(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    let told = Rc::new(Cell::new(0));
    let version = mutable_state_of(0);
    let (v, t) = (version.handle(), told.clone());
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        Leaf(Modifier.then_custom(AsksAgainElement(v.get(), t.clone())));
    });
    host.frame();
    version.set(1);
    host.frame();
    assert_eq!(told.get(), 1);
}

struct Consumes(&'static str, Rc<RefCell<Vec<&'static str>>>, Delegates);

impl ModifierNode for Consumes {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.2)
    }
    fn is_composition_local_consumer(&self) -> bool {
        true
    }
    fn on_composition_locals_changed(&mut self, _: &mut NodeContext<'_>) {
        self.1.borrow_mut().push(self.0);
    }
}

struct ConsumesElement(Rc<RefCell<Vec<&'static str>>>);

impl ModifierNodeElement for ConsumesElement {
    type Node = Consumes;
    fn create(&self) -> Consumes {
        let mut delegates = Delegates::new();
        delegates.delegate(Consumes("delegate", self.0.clone(), Delegates::new()));
        Consumes("owner", self.0.clone(), delegates)
    }
    fn update(&self, _: &mut Consumes) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/NodeKind.kt, autoInvalidateNodeIncludingDelegates
#[test]
fn a_consumer_and_its_delegate_are_each_told_of_new_locals() {
    static LOCAL: CompositionLocal<i32> = CompositionLocal::new_static("Local", || 0);
    let log = Rc::new(RefCell::new(Vec::new()));
    let value = mutable_state_of(0);
    let (v, l) = (value.handle(), log.clone());
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        let l = l.clone();
        CompositionLocalProvider(LOCAL.provides(v.get()), move || {
            Leaf(Modifier.then_custom(ConsumesElement(l.clone())));
        });
    });
    host.frame();
    value.set(1);
    host.frame();
    assert_eq!(*log.borrow(), ["owner", "delegate"]);
}

// ---- `invalidate_layer` -----------------------------------------------------------------------

struct InvalidatesLayer;
struct InvalidatesLayerNode;

impl ModifierNodeElement for InvalidatesLayer {
    type Node = InvalidatesLayerNode;
    fn create(&self) -> InvalidatesLayerNode {
        InvalidatesLayerNode
    }
    fn update(&self, _: &mut InvalidatesLayerNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for InvalidatesLayerNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        let handle = context.handle();
        HANDLES.with(|handles| handles.borrow_mut().insert("invalidates", handle));
    }
}

impl DrawModifierNode for InvalidatesLayerNode {
    fn draw(&mut self, _: &mut DrawScope<'_>) {
        log("drawn");
    }
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/NodeCoordinator.kt, invalidateLayer
#[test]
fn invalidating_the_layer_of_a_node_with_none_goes_up_to_the_one_around_it() {
    fresh();
    let mut host = UiHost::headless(200, 100);
    host.set_content(|| {
        Stack!(modifier = Modifier.graphics_layer(|_| {}), {
            Leaf(Modifier.size(10.dp()).then_custom(InvalidatesLayer));
        });
    });
    host.frame();
    take_log();
    assert!(!host.frame().drew, "nothing changed");
    handle("invalidates").invalidate_layer();
    assert!(host.needs_frame());
    let stats = host.frame();
    assert!(stats.drew);
    assert_eq!(take_log(), ["drawn"]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/NodeCoordinator.kt, invalidateLayer
#[test]
fn invalidating_the_layer_a_node_is_in_records_its_content_again() {
    fresh();
    let mut host = UiHost::headless(200, 100);
    host.set_content(|| {
        Leaf(Modifier.graphics_layer(|_| {}).size(10.dp()).then_custom(InvalidatesLayer));
    });
    host.frame();
    take_log();
    handle("invalidates").invalidate_layer();
    let stats = host.frame();
    assert!(stats.drew);
    assert_eq!(take_log(), ["drawn"]);
}

// ---- `invalidate_subtree` and semantics -------------------------------------------------------

struct Tagged(Rc<RefCell<&'static str>>);
struct TaggedNode(Rc<RefCell<&'static str>>);

impl ModifierNodeElement for Tagged {
    type Node = TaggedNode;
    fn create(&self) -> TaggedNode {
        TaggedNode(self.0.clone())
    }
    fn update(&self, _: &mut TaggedNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for TaggedNode {
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        let handle = context.handle();
        HANDLES.with(|handles| handles.borrow_mut().insert("tagged", handle));
    }
}

impl SemanticsModifierNode for TaggedNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.set_test_tag(*self.0.borrow());
    }
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/LayoutNode.kt, invalidateSubtree
#[test]
fn invalidating_a_subtree_computes_its_semantics_again() {
    fresh();
    let tag = Rc::new(RefCell::new("before"));
    let t = tag.clone();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        let t = t.clone();
        Stack!({
            Leaf(Modifier.then_custom(Tagged(t.clone())));
        });
    });
    host.frame();
    fn find(node: &SemanticsSnapshot, tag: &str) -> bool {
        node.config.get(&SemanticsProperties::TEST_TAG).is_some_and(|found| &**found == tag)
            || node.children.iter().any(|child| find(child, tag))
    }
    assert!(find(&host.semantics_tree(false), "before"));
    *tag.borrow_mut() = "after";
    handle("tagged").invalidate_subtree();
    let stats = host.frame();
    assert!(stats.semantics_changed);
    assert!(find(&host.semantics_tree(false), "after"));
}

// ---- Handlers' contexts, and observing from measuring and drawing -----------------------------

struct ObservesEverywhere {
    read: MutableState<i32>,
    handle: Option<NodeHandle>,
    in_measure: bool,
}

struct ObservesEverywhereElement {
    read: MutableState<i32>,
    in_measure: bool,
}

impl ModifierNodeElement for ObservesEverywhereElement {
    type Node = ObservesEverywhere;
    fn create(&self) -> ObservesEverywhere {
        ObservesEverywhere { read: self.read, handle: None, in_measure: self.in_measure }
    }
    fn update(&self, _: &mut ObservesEverywhere) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for ObservesEverywhere {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_observer(&mut self) -> Option<&mut dyn ObserverModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.handle = Some(context.handle());
        context.invalidate_measurement();
    }
}

impl ObserverModifierNode for ObservesEverywhere {
    fn on_observed_reads_changed(&mut self, _: &mut NodeContext<'_>) {
        log("told");
    }
}

impl LayoutModifierNode for ObservesEverywhere {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        if self.in_measure
            && let Some(handle) = self.handle
        {
            let read = self.read;
            handle.observe_reads(|| read.get());
        }
        let placeable = scope.measure(content, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

impl DrawModifierNode for ObservesEverywhere {
    fn draw(&mut self, _: &mut DrawScope<'_>) {
        if !self.in_measure
            && let Some(handle) = self.handle
        {
            let read = self.read;
            handle.observe_reads(|| read.get());
        }
    }
}

fn observing_host(in_measure: bool, value: &OwnedState<i32>) -> UiHost {
    fresh();
    let read = value.handle();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        Leaf(Modifier.then_custom(ObservesEverywhereElement { read, in_measure }).size(10.dp()));
    });
    host.frame();
    host.frame();
    take_log();
    host
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Background.kt, getOutline
#[test]
fn a_node_observes_reads_while_it_draws() {
    let value = mutable_state_of(0);
    let mut host = observing_host(false, &value);
    value.set(1);
    host.frame();
    assert_eq!(take_log(), ["told"]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/ObserverModifierNode.kt, observeReads
#[test]
fn a_node_observes_reads_while_it_measures() {
    let value = mutable_state_of(0);
    let mut host = observing_host(true, &value);
    value.set(1);
    let stats = host.frame();
    assert_eq!(take_log(), ["told"]);
    // Nested: the read is the observation's, not the measurement's around it.
    assert!(!stats.measured);
}

struct UsesItsContextNode {
    observed: MutableState<i32>,
}

struct UsesItsContextElement(MutableState<i32>);

impl ModifierNodeElement for UsesItsContextElement {
    type Node = UsesItsContextNode;
    fn create(&self) -> UsesItsContextNode {
        UsesItsContextNode { observed: self.0 }
    }
    fn update(&self, _: &mut UsesItsContextNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for UsesItsContextNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_observer(&mut self) -> Option<&mut dyn ObserverModifierNode> {
        Some(self)
    }
}

impl ObserverModifierNode for UsesItsContextNode {
    fn on_observed_reads_changed(&mut self, _: &mut NodeContext<'_>) {
        log("told");
    }
}

impl PointerInputModifierNode for UsesItsContextNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Main && event.kind == PointerEventType::Press {
            let observed = self.observed;
            context.observe_reads(|| observed.get());
            let handle = context.handle();
            log(format!("attached {}", handle.is_attached()));
            let mut above = 0;
            context.visit_ancestors(NodeKinds::ANY, false, &mut |_, _| {
                above += 1;
                std::ops::ControlFlow::Continue(())
            });
            log(format!("above {}", above > 0));
        }
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, onPointerEvent
#[test]
fn a_handler_s_context_observes_gets_a_handle_and_traverses() {
    fresh();
    let value = mutable_state_of(0);
    let read = value.handle();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        Leaf(nodes_before().size(50.dp()).then_custom(UsesItsContextElement(read)));
    });
    host.frame();
    take_log();
    host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    assert_eq!(take_log(), ["attached true", "above true"]);
    value.set(1);
    host.frame();
    assert_eq!(take_log(), ["told"]);
}

fn nodes_before() -> Modifier {
    Modifier.then_custom(element(|| Mod::plain("before")))
}

// ---- Built-in elements' nodes ----------------------------------------------------------------

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, delegate
// A built-in element's node with a lifecycle (`observes_reads`) may delegate, as the clickable
// will.
#[test]
fn a_node_with_a_lifecycle_delegates_without_a_user_defined_element() {
    struct Builtin(Delegates);
    impl ModifierNode for Builtin {
        fn observes_reads(&self) -> bool {
            true
        }
        fn delegates(&mut self) -> Option<&mut Delegates> {
            Some(&mut self.0)
        }
    }
    let mut delegates = Delegates::new();
    delegates.delegate(Mod::draw("delegate"));
    let mut node = Builtin(delegates);
    // The kinds a chain would take it with, unobserved as a built-in element's.
    assert!(chain_entry_kinds_for_test(&mut node, false).any(NodeKinds::DRAW));
    struct Plain(Delegates);
    impl ModifierNode for Plain {
        fn delegates(&mut self) -> Option<&mut Delegates> {
            Some(&mut self.0)
        }
    }
    let mut plain = Plain(Delegates::new());
    plain.0.delegate(Mod::draw("delegate"));
    assert!(!chain_entry_kinds_for_test(&mut plain, false).any(NodeKinds::DRAW));
}

// ---- Forgetting ------------------------------------------------------------------------------

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/ObserverModifierNode.kt, ObserverNodeOwnerScope
#[test]
fn a_released_node_and_a_released_delegate_leave_no_observation() {
    fresh();
    let value = mutable_state_of(0);
    let shown = mutable_state_of(true);
    let s = shown.handle();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        if s.get() {
            Leaf(Modifier.then_custom(element(|| Mod::wrapping("owner", Mod::plain("delegate")))));
        }
    });
    host.frame();
    let read = value.handle();
    for label in ["owner", "delegate"] {
        handle(label)
            .with_node(|_: &mut Mod, context| context.observe_reads(|| read.get()))
            .expect("the tree is free");
    }
    assert_eq!(host.with_tree(|tree| tree.observing_nodes_for_test()), 2);
    shown.set(false);
    host.frame();
    assert_eq!(host.with_tree(|tree| tree.observing_nodes_for_test()), 0);
}

#[test]
fn a_handle_s_observation_is_forgotten_with_its_node() {
    fresh();
    let value = mutable_state_of(0);
    let shown = mutable_state_of(true);
    let s = shown.handle();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        if s.get() {
            Leaf(Modifier.then_custom(element(|| Mod::plain("node"))));
        }
    });
    host.frame();
    let read = value.handle();
    handle("node").observe_reads(|| read.get());
    assert_eq!(host.with_tree(|tree| tree.observing_nodes_for_test()), 1);
    shown.set(false);
    host.frame();
    assert_eq!(host.with_tree(|tree| tree.observing_nodes_for_test()), 0);
}

// ---- Reaching the delegator (docs/parity/design/modifier-nodes.md §3.7) ------------------------

#[derive(Default)]
struct SharedWithOwner {
    owner: Cell<Option<NodeHandle>>,
    pressed: Cell<bool>,
}

struct Pressable(Rc<SharedWithOwner>);

impl ModifierNode for Pressable {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for Pressable {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Main && event.kind == PointerEventType::Press {
            self.0.pressed.set(true);
            let owner = self.0.owner.get().expect("the owner attached");
            owner.invalidate_draw();
            // Its code, once the tree is free.
            context.side_effect(move || {
                let told = owner.with_node(|owner: &mut PressableOwner, _| {
                    log(format!("told pressed {}", owner.shared.pressed.get()));
                });
                assert!(told.is_some(), "the tree is free in a side effect");
            });
        }
    }
}

struct PressableOwner {
    shared: Rc<SharedWithOwner>,
    delegates: Delegates,
}

struct PressableElement;

impl ModifierNodeElement for PressableElement {
    type Node = PressableOwner;
    fn create(&self) -> PressableOwner {
        let shared = Rc::new(SharedWithOwner::default());
        let mut delegates = Delegates::new();
        delegates.delegate(Pressable(shared.clone()));
        PressableOwner { shared, delegates }
    }
    fn update(&self, _: &mut PressableOwner) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for PressableOwner {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.shared.owner.set(Some(context.handle()));
    }
}

impl DrawModifierNode for PressableOwner {
    fn draw(&mut self, _: &mut DrawScope<'_>) {
        log(format!("drawn pressed {}", self.shared.pressed.get()));
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, AbstractClickableNode
#[test]
fn a_delegate_reaches_its_owner_through_what_they_share_and_a_handle() {
    fresh();
    let mut host = UiHost::headless(200, 100);
    host.set_content(|| Leaf(Modifier.size(50.dp()).then_custom(PressableElement)));
    host.frame();
    assert_eq!(take_log(), ["drawn pressed false"]);
    host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    assert!(host.needs_frame());
    host.frame();
    assert_eq!(take_log(), ["told pressed true", "drawn pressed true"]);
}
