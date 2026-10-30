//! `DelegatingNode`: `Delegates`, dispatch by kind, delegates' lifecycle
//! (docs/parity/design/modifier-nodes.md §3.7, step MN-5).

use crate::support;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
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
}

impl Mod {
    fn plain(label: &'static str) -> Mod {
        Mod { label, draw: false, layout: false, semantics: false, delegates: Delegates::new() }
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
        node.delegates.delegate(wrapped);
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
        scope.place(&placeable, 0, 0);
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

/// As the test's `asKind(Nodes.Draw)`: the node if it draws, else the first of its delegates that
/// does, breadth first, the newest first (Compose's list of delegates has the newest at its head).
fn as_draw_kind(node: &mut Mod) -> Option<&'static str> {
    let mut queue: VecDeque<&mut Mod> = VecDeque::new();
    queue.push_back(node);
    while let Some(next) = queue.pop_front() {
        if next.draw {
            return Some(next.label);
        }
        let mut delegates: Vec<&mut Mod> =
            next.delegates.nodes().filter_map(|node| node.downcast_mut::<Mod>()).collect();
        delegates.reverse();
        queue.extend(delegates);
    }
    None
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
    fn on_focus_changed(&mut self, focused: bool, _: &mut EventContext<'_>) {
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
    assert_eq!(take_log(), ["focus first true"]);
    host.send_key(KeyEvent::down(Key::Enter));
    assert_eq!(take_log(), ["key first", "key second"]);
}

#[test]
fn a_delegate_s_semantics_are_its_owner_s() {
    let host = layout(vec![element(|| Mod::wrapping("owner", Mod::semantics("tagged")))]);
    let tree = host.semantics_tree(false);
    fn has_tag(node: &SemanticsNode, tag: &str) -> bool {
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
    fn on_globally_positioned(&mut self, coordinates: LayoutCoordinates) -> Option<Action> {
        self.0.borrow_mut().push(coordinates.position_in_root());
        None
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
