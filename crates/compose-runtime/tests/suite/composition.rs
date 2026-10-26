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

// ---- Reports and points ----------------------------------------------------------------------

/// A report, as the mock's `Report`: mutable, compared by identity.
struct Report {
    from: std::cell::RefCell<String>,
    to: std::cell::RefCell<String>,
}

type ReportRef = std::rc::Rc<Report>;

fn report(from: &str, to: &str) -> ReportRef {
    std::rc::Rc::new(Report {
        from: std::cell::RefCell::new(from.into()),
        to: std::cell::RefCell::new(to.into()),
    })
}

impl Report {
    fn from(&self) -> String {
        self.from.borrow().clone()
    }
    fn to(&self) -> String {
        self.to.borrow().clone()
    }
}

/// Jim reports to Sally, Rob to Alice, Clark to Lois.
fn reports() -> Vec<ReportRef> {
    vec![report("Jim", "Sally"), report("Rob", "Alice"), report("Clark", "Lois")]
}

#[composable]
fn ReportsTo(report: ReportRef) {
    Text(&report.from());
    Text("reports to");
    Text(&report.to());
}

#[composable]
fn ReportsReport(reports: std::rc::Rc<Vec<ReportRef>>) {
    Linear!({
        Repeated(reports.iter().cloned(), identity, ReportsTo);
    });
}

fn expect_reports_to(v: &mut Expected, report: &Report) {
    v.text(&report.from());
    v.text("reports to");
    v.text(&report.to());
}

fn expect_reports_report(v: &mut Expected, reports: &[ReportRef]) {
    v.linear(|v| {
        for report in reports {
            expect_reports_to(v, report);
        }
    });
}

/// A point, as the mock's data class `Point`: compared by value.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, ComposeArg)]
struct Point {
    x: i32,
    y: i32,
}

fn point(x: i32, y: i32) -> Point {
    Point { x, y }
}

#[composable]
fn PointView(point: Point) {
    Text(&format!("X: {} Y: {}", point.x, point.y));
}

#[composable]
fn Points(points: std::rc::Rc<Vec<Point>>) {
    Repeated(points.iter().copied(), |point| *point, PointView);
}

fn expect_points(v: &mut Expected, points: &[Point]) {
    for point in points {
        v.text(&format!("X: {} Y: {}", point.x, point.y));
    }
}

/// A value tests change between passes, as a captured Kotlin `var`.
type Var<T> = std::rc::Rc<std::cell::RefCell<T>>;

fn var<T>(value: T) -> Var<T> {
    std::rc::Rc::new(std::cell::RefCell::new(value))
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testSeveralArbitraryMoves
#[test]
fn keyed_items_follow_several_arbitrary_moves() {
    #[composable]
    fn Content(list: MutableState<Vec<i32>>) {
        for item in list.get() {
            key(item, || Text(&format!("Item {item}")));
        }
    }
    let mut test = composition_test();
    let list = mutable_state_of((0..10).collect::<Vec<i32>>());
    let l = list.handle();
    test.compose(move || Content(l));
    test.validate(move |v| {
        for item in l.get() {
            v.text(&format!("Item {item}"));
        }
    });
    // A deterministic shuffle (Compose's uses `Random(1337)`).
    let mut seed: u64 = 1337;
    for _ in 0..11 {
        list.edit(|items| {
            for index in (1..items.len()).rev() {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                items.swap(index, (seed >> 33) as usize % (index + 1));
            }
        });
        test.expect_changes();
        test.revalidate();
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testConditionalNode
#[test]
fn a_conditional_node_comes_and_goes() {
    #[composable]
    fn Content(condition: MutableState<bool>) {
        Linear!({
            Text("A");
            Wrap!({
                if condition.get() {
                    Linear!({ Text("B") });
                }
            });
        });
    }
    let mut test = composition_test();
    let condition = mutable_state_of(false);
    let c = condition.handle();
    test.compose(move || Content(c));
    test.validate(move |v| {
        v.linear(|v| {
            v.text("A");
            if c.get() {
                v.linear(|v| v.text("B"));
            }
        })
    });
    for _ in 0..10 {
        condition.update(|condition| !condition);
        test.advance();
        test.revalidate();
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testComposeCompositionWithMultipleRoots
#[test]
fn a_composable_with_several_root_views() {
    let mut test = composition_test();
    let reports = std::rc::Rc::new(reports());
    let r = reports.clone();
    test.compose(move || ReportsReport(r.clone()));
    test.validate(move |v| expect_reports_report(v, &reports));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testMoveCompositionWithMultipleRoots
#[test]
fn composables_with_several_root_views_move() {
    let mut test = composition_test();
    let all = reports();
    let reports = var(std::rc::Rc::new(all.clone()));
    let scope = ScopeHolder::new();
    let (r, s) = (reports.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        ReportsReport(r.borrow().clone());
    });
    *reports.borrow_mut() = std::rc::Rc::new(vec![all[0].clone(), all[2].clone(), all[1].clone()]);
    scope.invalidate();
    test.expect_changes();
    let shown = reports.clone();
    test.validate(move |v| expect_reports_report(v, &shown.borrow()));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testReplace
#[test]
fn a_branch_replaces_the_other() {
    #[composable]
    fn Composition(include_a: std::rc::Rc<std::cell::Cell<bool>>, scope: ScopeHolder) {
        scope.capture();
        Text("Before");
        if include_a.get() {
            Linear!({ Text("A") });
        } else {
            Edit("B".into());
        }
        Text("After");
    }
    let mut test = composition_test();
    let include_a = std::rc::Rc::new(std::cell::Cell::new(true));
    let scope = ScopeHolder::new();
    let (i, s) = (include_a.clone(), scope.clone());
    test.compose(move || Composition(i.clone(), s.clone()));
    let shown = include_a.clone();
    test.validate(move |v| {
        v.text("Before");
        if shown.get() {
            v.linear(|v| v.text("A"));
        } else {
            v.edit("B");
        }
        v.text("After");
    });
    include_a.set(false);
    scope.invalidate();
    test.expect_changes();
    test.revalidate();
    include_a.set(true);
    scope.invalidate();
    test.expect_changes();
    test.revalidate();
    scope.invalidate();
    test.expect_no_changes();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testInsertWithMultipleRoots
#[test]
fn an_insertion_reaches_every_repeated_root() {
    #[composable]
    fn TextOf(c: char) {
        Text(&c.to_string());
    }
    #[composable]
    fn Chars(chars: std::rc::Rc<Vec<char>>) {
        Repeated(chars.iter().copied(), |c| *c, TextOf);
    }
    let mut test = composition_test();
    let chars = var(std::rc::Rc::new(vec!['a', 'b', 'c']));
    let scope = ScopeHolder::new();
    let (c, s) = (chars.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        let chars = c.borrow().clone();
        Chars(chars.clone());
        Chars(chars.clone());
        Chars(chars);
    });
    let shown = chars.clone();
    test.validate(move |v| {
        for _ in 0..3 {
            for c in shown.borrow().iter() {
                v.text(&c.to_string());
            }
        }
    });
    *chars.borrow_mut() = std::rc::Rc::new(vec!['a', 'b', 'x', 'c']);
    scope.invalidate();
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testSimpleSkipping
#[test]
fn unchanged_points_skip() {
    let mut test = composition_test();
    let points = std::rc::Rc::new(vec![point(1, 2), point(2, 3)]);
    let scope = ScopeHolder::new();
    let (p, s) = (points.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        Points(p.clone());
    });
    test.validate(move |v| expect_points(v, &points));
    scope.invalidate();
    test.expect_no_changes();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testMovingMemoization
#[test]
fn moved_points_keep_their_views() {
    let mut test = composition_test();
    let points = var(std::rc::Rc::new(vec![point(1, 2), point(2, 3), point(4, 5), point(6, 7)]));
    let scope = ScopeHolder::new();
    let (p, s) = (points.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        Points(p.borrow().clone());
    });
    let shown = points.clone();
    test.validate(move |v| expect_points(v, &shown.borrow()));
    *points.borrow_mut() =
        std::rc::Rc::new(vec![point(1, 2), point(4, 5), point(2, 3), point(6, 7)]);
    scope.invalidate();
    test.expect_changes();
    test.revalidate();
}

#[composable]
fn Reporter(report: Option<ReportRef>) {
    match report {
        Some(report) => ReportsTo(report),
        None => Text("no report to report"),
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testComponent
#[test]
fn a_component_shows_each_report() {
    #[composable]
    fn ReportersReport(reports: std::rc::Rc<Vec<ReportRef>>) {
        Linear!({
            Repeated(reports.iter().cloned(), identity, |report| Reporter(Some(report)));
        });
    }
    let mut test = composition_test();
    let reports = std::rc::Rc::new(reports());
    let r = reports.clone();
    test.compose(move || ReportersReport(r.clone()));
    test.validate(move |v| expect_reports_report(v, &reports));
    test.expect_no_changes();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testComposeTwoAttributeComponent
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testComposeThreeAttributeComponent
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testComposeFourOrMoreAttributeComponent
#[test]
fn components_take_several_arguments_with_defaults() {
    #[composable]
    fn Four(
        #[default(1)] first: i32,
        #[default(2)] second: i32,
        #[default(3)] third: i32,
        #[default(4)] fourth: i32,
    ) {
        Linear!({ Text(&format!("{first} {second} {third} {fourth}")) });
    }
    let mut test = composition_test();
    test.compose(|| {
        Four!(first = 41, second = 42);
        Four!(first = 41, second = 42, third = 43);
        Four!(first = 41, second = 42, third = 43, fourth = 44);
    });
    test.validate(|v| {
        v.linear(|v| v.text("41 42 3 4"));
        v.linear(|v| v.text("41 42 43 4"));
        v.linear(|v| v.text("41 42 43 44"));
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testSkippingACall
#[test]
fn a_call_added_at_the_end_composes_alone() {
    #[composable]
    fn Show(value: i32) {
        Linear!({ Text(&value.to_string()) });
        Linear!({ Text("value") });
    }
    #[composable]
    fn Test(show_three: bool) {
        Show(1);
        Show(2);
        if show_three {
            Show(3);
        }
    }
    let mut test = composition_test();
    let show_three = std::rc::Rc::new(std::cell::Cell::new(false));
    let scope = ScopeHolder::new();
    let (t, s) = (show_three.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        Test(t.get());
    });
    let shown = show_three.clone();
    test.validate(move |v| {
        let count = if shown.get() { 3 } else { 2 };
        for value in 1..=count {
            v.linear(|v| v.text(&value.to_string()));
            v.linear(|v| v.text("value"));
        }
    });
    show_three.set(true);
    scope.invalidate();
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testSkippingNestedLambda
#[test]
fn an_item_renderer_lambda_skips_its_unchanged_item() {
    #[derive(Clone, Copy, PartialEq, Debug, ComposeArg)]
    struct ListViewItem {
        id: i32,
    }
    thread_local! {
        static ITEM_RENDERER_CALLS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        static SCROLLING_LIST_CALLS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }
    fn bump(counter: &'static std::thread::LocalKey<std::cell::Cell<u32>>) {
        counter.with(|count| count.set(count.get() + 1));
    }
    #[composable]
    fn ItemRenderer(view_item: ListViewItem) {
        bump(&ITEM_RENDERER_CALLS);
        Text(&view_item.id.to_string());
    }
    #[composable]
    fn ListContentItem(view_item: ListViewItem, item_renderer: impl Fn(ListViewItem) + 'static) {
        item_renderer(view_item);
    }
    #[composable]
    fn ListContent(
        view_items: std::rc::Rc<Vec<ListViewItem>>,
        item_renderer: impl Fn(ListViewItem) + 'static,
    ) {
        for view_item in view_items.iter() {
            ListContentItem(*view_item, item_renderer.clone());
        }
    }
    #[composable]
    fn ScrollingList(item_renderer: impl Fn(ListViewItem) + 'static) {
        let view_items =
            remember(|| std::rc::Rc::new(vec![ListViewItem { id: 0 }, ListViewItem { id: 1 }]));
        ListContent(view_items, item_renderer);
    }
    #[composable]
    fn TestSkippingContent(data: State<i32>) {
        ScrollingList!(|view_item: ListViewItem| {
            Text(&data.get().to_string());
            bump(&SCROLLING_LIST_CALLS);
            ItemRenderer(view_item);
        });
    }
    let mut test = composition_test();
    let data = mutable_state_of(0);
    let d = data.as_state();
    test.compose(move || TestSkippingContent(d));
    for _ in 0..3 {
        data.update(|data| data + 1);
        test.advance();
    }
    let (items, list) = (ITEM_RENDERER_CALLS.get(), SCROLLING_LIST_CALLS.get());
    assert!(items < list, "item renderer ran {items} times, the list's lambda {list}");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testComponentWithVarConstructorParameter
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testComponentWithValConstructorParameter
#[test]
fn a_component_follows_its_argument() {
    #[composable]
    fn One(first: i32) {
        Text(&first.to_string());
    }
    #[composable]
    fn CallOne(value: i32) {
        One(value);
    }
    let mut test = composition_test();
    let value = std::rc::Rc::new(std::cell::Cell::new(42));
    let scope = ScopeHolder::new();
    let (v, s) = (value.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        CallOne(v.get());
    });
    test.validate(|v| v.text("42"));
    value.set(43);
    scope.invalidate();
    test.expect_changes();
    test.validate(|v| v.text("43"));
    scope.invalidate();
    test.expect_no_changes();
}

/// `Reporter` that hands out its scope when it shows Lois.
#[composable]
fn LoisReporter(
    report: ReportRef,
    lois_scope: ScopeHolder,
    key_value: std::rc::Rc<std::cell::Cell<i32>>,
) {
    if report.from() == "Lois" || report.to() == "Lois" {
        lois_scope.capture();
    }
    key(key_value.get(), || {
        Text(&report.from());
        Text("reports to");
        Text(&report.to());
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testComposePartOfTree
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRecomposeWithReplace
#[test]
fn one_scope_of_a_tree_recomposes_alone() {
    for change_key in [false, true] {
        #[composable]
        fn Content(
            reports: std::rc::Rc<Vec<ReportRef>>,
            lois_scope: ScopeHolder,
            key_value: std::rc::Rc<std::cell::Cell<i32>>,
        ) {
            Linear!({
                Repeated(reports.iter().cloned(), identity, |report| {
                    LoisReporter(report, lois_scope.clone(), key_value.clone())
                });
            });
        }
        let mut test = composition_test();
        let r = report("Lois", "Perry");
        let mut all = reports();
        all.push(r.clone());
        let all = std::rc::Rc::new(all);
        let lois_scope = ScopeHolder::new();
        let key_value = std::rc::Rc::new(std::cell::Cell::new(0));
        let (a, l, k) = (all.clone(), lois_scope.clone(), key_value.clone());
        test.compose(move || Content(a.clone(), l.clone(), k.clone()));
        let shown = all.clone();
        test.validate(move |v| expect_reports_report(v, &shown));
        test.expect_no_changes();
        // Demote Perry.
        *r.from.borrow_mut() = "Perry".into();
        *r.to.borrow_mut() = "Lois".into();
        if change_key {
            // A new group in the component.
            key_value.set(2);
        }
        lois_scope.invalidate();
        test.expect_changes();
        test.revalidate();
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testInvalidationAfterRemoval
#[test]
fn invalidating_a_removed_scope_changes_nothing() {
    #[composable]
    fn Content(
        reports: std::rc::Rc<Vec<ReportRef>>,
        include_lois: std::rc::Rc<std::cell::Cell<bool>>,
        lois_scope: ScopeHolder,
        scope: ScopeHolder,
    ) {
        scope.capture();
        let key_value = std::rc::Rc::new(std::cell::Cell::new(0));
        Linear!({
            Repeated(reports.iter().cloned(), identity, |report| {
                let is_lois = report.from() == "Lois" || report.to() == "Lois";
                if include_lois.get() || !is_lois {
                    LoisReporter(report, lois_scope.clone(), key_value.clone());
                }
            });
        });
    }
    let mut test = composition_test();
    let r = report("Lois", "Perry");
    let mut all = reports();
    all.push(r.clone());
    let all = std::rc::Rc::new(all);
    let include_lois = std::rc::Rc::new(std::cell::Cell::new(true));
    let (lois_scope, scope) = (ScopeHolder::new(), ScopeHolder::new());
    let (a, i, l, s) = (all.clone(), include_lois.clone(), lois_scope.clone(), scope.clone());
    test.compose(move || Content(a.clone(), i.clone(), l.clone(), s.clone()));
    let shown = all.clone();
    test.validate(move |v| expect_reports_report(v, &shown));
    include_lois.set(false);
    scope.invalidate();
    test.expect_changes();
    let shown = all.clone();
    test.validate(move |v| expect_reports_report(v, &shown[..2]));
    // Lois's scope is gone.
    lois_scope.invalidate();
    test.expect_no_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testSimpleRemember
#[test]
fn a_remembered_value_survives_recomposition() {
    #[composable]
    fn Test(value: i32, count: Counter, scope: ScopeHolder) {
        scope.capture();
        let w = remember(move || {
            count.increment();
            value
        });
        Text(&format!("value = {w}"));
    }
    let mut test = composition_test();
    let (count, scope) = (Counter::new(), ScopeHolder::new());
    let (c, s) = (count.clone(), scope.clone());
    test.compose(move || Test(1, c.clone(), s.clone()));
    test.validate(|v| v.text("value = 1"));
    assert_eq!(count.get(), 1);
    scope.invalidate();
    test.expect_no_changes();
    assert_eq!(count.get(), 1, "the remembered instance stays");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberOneParameter
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberTwoParameters
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberThreeParameters
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberFourParameters
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberFiveParameters
#[test]
fn a_keyed_remember_follows_one_to_five_keys() {
    #[composable]
    fn Test(keys: [i32; 5], arity: usize, count: Counter) {
        let w = match arity {
            1 => remember_keyed(keys[0], || keys),
            2 => remember_keyed((keys[0], keys[1]), || keys),
            3 => remember_keyed((keys[0], keys[1], keys[2]), || keys),
            4 => remember_keyed((keys[0], keys[1], keys[2], keys[3]), || keys),
            _ => remember_keyed((keys[0], keys[1], keys[2], keys[3], keys[4]), || keys),
        };
        // Counted where it's computed: a new value is a new array.
        remember_keyed(w, || {
            count.increment();
        });
        Text(&format!("{:?}", &w[..arity]));
    }
    for arity in 1..=5 {
        let mut test = composition_test();
        let keys = std::rc::Rc::new(std::cell::Cell::new([1, 2, 3, 4, 5]));
        let (count, scope) = (Counter::new(), ScopeHolder::new());
        let (k, c, s) = (keys.clone(), count.clone(), scope.clone());
        test.compose(move || {
            s.capture();
            Test(k.get(), arity, c.clone());
        });
        // The last key changes.
        let mut changed = keys.get();
        changed[arity - 1] += 1;
        keys.set(changed);
        scope.invalidate();
        test.expect_changes();
        let shown = keys.clone();
        test.validate(move |v| v.text(&format!("{:?}", &shown.get()[..arity])));
        scope.invalidate();
        test.expect_no_changes();
        test.revalidate();
        assert_eq!(count.get(), 2, "{arity} keys: computed again once");
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberVarargParametersDropped
#[test]
fn a_remember_keyed_on_fewer_keys_computes_anew() {
    // Compose's varargs: an array; one of another length is a change. Compose also checks the
    // slot table freed the dropped keys' slots: here the keys are one slot.
    #[composable]
    fn Content(keys: MutableState<std::rc::Rc<Vec<i32>>>, remembered: Log) {
        let keys = keys.get();
        let value = match keys.len() {
            5 => remember_keyed([keys[0], keys[1], keys[2], keys[3], keys[4]], || {
                std::rc::Rc::new(())
            }),
            4 => remember_keyed([keys[0], keys[1], keys[2], keys[3]], || std::rc::Rc::new(())),
            3 => remember_keyed([keys[0], keys[1], keys[2]], || std::rc::Rc::new(())),
            _ => remember_keyed([0; 0], || std::rc::Rc::new(())),
        };
        remembered.push(format!("{:p}", std::rc::Rc::as_ptr(&value)));
    }
    let mut test = composition_test();
    let keys = mutable_state_of_with(std::rc::Rc::new(vec![1, 2, 3, 4, 5]), NeverEqualPolicy);
    let remembered = Log::new();
    let (k, r) = (keys.handle(), remembered.clone());
    test.compose(move || Content(k, r.clone()));
    let unique = || {
        let mut entries = remembered.entries();
        entries.sort();
        entries.dedup();
        entries.len()
    };
    assert_eq!(unique(), 1);
    keys.set(std::rc::Rc::new(vec![1, 2, 3, 4, 5]));
    test.advance();
    assert_eq!(unique(), 1, "the same keys keep the value");
    keys.set(std::rc::Rc::new(vec![2, 3, 4, 5]));
    test.expect_changes();
    assert_eq!(unique(), 2);
    keys.set(std::rc::Rc::new(vec![3, 4, 5]));
    test.expect_changes();
    assert_eq!(unique(), 3);
    keys.set(std::rc::Rc::new(vec![]));
    test.expect_changes();
    assert_eq!(unique(), 4);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testInsertGroupInContainer
#[test]
fn groups_inserted_in_a_container_one_by_one() {
    #[composable]
    fn Composition(values: MutableState<Vec<i32>>) {
        Linear!({
            for value in values.get() {
                Text(&value.to_string());
            }
        });
    }
    let mut test = composition_test();
    let values = mutable_state_list_from([0]);
    let v = values.handle();
    test.compose(move || Composition(v));
    test.validate(move |e| {
        e.linear(|e| v.get().iter().for_each(|value| e.text(&value.to_string())))
    });
    for i in 1..=10 {
        values.edit(|values| values.push(i));
        test.expect_changes();
        test.revalidate();
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testInsertInGroups
#[test]
fn views_inserted_in_the_middle_of_a_container() {
    #[composable]
    fn Composition(three_visible: MutableState<bool>) {
        Linear!({
            Text("one");
            Text("two");
            if three_visible.get() {
                Text("three");
                Text("four");
            }
            Linear!({ Text("five") });
        });
    }
    let mut test = composition_test();
    let three_visible = mutable_state_of(false);
    let t = three_visible.handle();
    test.compose(move || Composition(t));
    test.validate(move |v| {
        v.linear(|v| {
            v.text("one");
            v.text("two");
            if t.get() {
                v.text("three");
                v.text("four");
            }
            v.linear(|v| v.text("five"));
        })
    });
    three_visible.set(true);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testStartJoin
#[test]
fn a_container_follows_its_texts_state() {
    #[composable]
    fn Composition(text: MutableState<String>) {
        Linear!({ Text(&text.get()) });
    }
    let mut test = composition_test();
    let text = mutable_state_of(String::from("Starting"));
    let t = text.handle();
    test.compose(move || Composition(t));
    test.validate(move |v| v.linear(|v| v.text(&t.get())));
    text.set("Ending".into());
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testInvalidateJoin_End
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testInvalidateJoin_Start
#[test]
fn a_removed_nested_text_and_a_changed_one_in_one_pass() {
    for nested_first in [false, true] {
        #[composable]
        fn Composition(
            text: MutableState<String>,
            include_nested: MutableState<bool>,
            nested_first: bool,
        ) {
            Linear!({
                if nested_first && include_nested.get() {
                    Text(&format!("Nested in {}", text.get()));
                }
                Text(&text.get());
                if !nested_first && include_nested.get() {
                    Text(&format!("Nested in {}", text.get()));
                }
            });
        }
        let mut test = composition_test();
        let text = mutable_state_of(String::from("Starting"));
        let include_nested = mutable_state_of(true);
        let (t, i) = (text.handle(), include_nested.handle());
        test.compose(move || Composition(t, i, nested_first));
        test.validate(move |v| {
            v.linear(|v| {
                let nested = format!("Nested in {}", t.get());
                if nested_first && i.get() {
                    v.text(&nested);
                }
                v.text(&t.get());
                if !nested_first && i.get() {
                    v.text(&nested);
                }
            })
        });
        text.set("Ending".into());
        include_nested.set(false);
        test.expect_changes();
        test.revalidate();
        test.expect_no_changes();
        test.revalidate();
    }
}

// ---- Remember observers --------------------------------------------------------------------

/// The order in which tracked observers hear their callbacks, as the tests' `var order`.
#[derive(Clone, Default)]
struct Orders(
    std::rc::Rc<std::cell::RefCell<Vec<std::rc::Rc<TrackedState>>>>,
    std::rc::Rc<std::cell::Cell<i32>>,
);

/// A remember observer counting enters and leaves and recording their order, as the tests'
/// `object : RememberObserver, Counted, Ordered, Named`.
struct TrackedState {
    name: String,
    data: i32,
    count: std::cell::Cell<i32>,
    remember_order: std::cell::Cell<i32>,
    forget_order: std::cell::Cell<i32>,
    abandoned: std::cell::Cell<bool>,
    order: std::rc::Rc<std::cell::Cell<i32>>,
}

#[derive(Clone)]
struct Tracked(std::rc::Rc<TrackedState>);

impl RememberObserver for Tracked {
    fn on_remembered(&self) {
        let state = &self.0;
        assert_eq!(state.remember_order.get(), -1, "{}: one on_remembered", state.name);
        state.remember_order.set(state.order.get());
        state.order.set(state.order.get() + 1);
        state.count.set(state.count.get() + 1);
    }
    fn on_forgotten(&self) {
        let state = &self.0;
        assert_eq!(state.forget_order.get(), -1, "{}: one on_forgotten", state.name);
        state.forget_order.set(state.order.get());
        state.order.set(state.order.get() + 1);
        state.count.set(state.count.get() - 1);
    }
    fn on_abandoned(&self) {
        assert_eq!(self.0.count.get(), 0, "{}: on_abandoned after on_remembered", self.0.name);
        self.0.abandoned.set(true);
    }
}

impl ComposeArg for Orders {
    fn same(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Orders {
    fn new() -> Orders {
        Orders::default()
    }

    /// A new tracked observer.
    fn track(&self, name: &str, data: i32) -> Tracked {
        let state = std::rc::Rc::new(TrackedState {
            name: name.into(),
            data,
            count: std::cell::Cell::new(0),
            remember_order: std::cell::Cell::new(-1),
            forget_order: std::cell::Cell::new(-1),
            abandoned: std::cell::Cell::new(false),
            order: self.1.clone(),
        });
        self.0.borrow_mut().push(state.clone());
        Tracked(state)
    }

    /// Remembers a new tracked observer here.
    fn remember(&self, name: &str) {
        let orders = self.clone();
        let name = name.to_string();
        remember_observer(move || orders.track(&name, 0));
    }

    fn all_counts(&self, count: i32) -> bool {
        self.0.borrow().iter().all(|state| state.count.get() == count)
    }

    fn names_by(&self, order: fn(&TrackedState) -> i32) -> Vec<String> {
        let mut states: Vec<_> =
            self.0.borrow().iter().filter(|state| order(state) >= 0).cloned().collect();
        states.sort_by_key(|state| order(state));
        states.iter().map(|state| state.name.clone()).collect()
    }

    fn remembered(&self) -> Vec<String> {
        self.names_by(|state| state.remember_order.get())
    }

    fn forgotten(&self) -> Vec<String> {
        self.names_by(|state| state.forget_order.get())
    }
}

/// An observer shared by several `remember`s, as an object Kotlin remembers twice.
#[derive(Clone)]
struct Shared(std::rc::Rc<std::cell::Cell<i32>>);

impl RememberObserver for Shared {
    fn on_remembered(&self) {
        self.0.set(self.0.get() + 1);
    }
    fn on_forgotten(&self) {
        self.0.set(self.0.get() - 1);
    }
    fn on_abandoned(&self) {
        assert_eq!(self.0.get(), 0, "on_abandoned after on_remembered");
    }
}

impl ComposeArg for Shared {
    fn same(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

fn shared() -> Shared {
    Shared(std::rc::Rc::new(std::cell::Cell::new(0)))
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberObserver_Remember_Simple
#[test]
fn a_remembered_observer_hears_on_remembered_once() {
    #[composable]
    fn Composition(observer: Shared, scope: ScopeHolder) {
        Linear!({
            scope.capture();
            let observer = observer.clone();
            remember_observer(move || observer);
            Text("Some text");
        });
    }
    let mut test = composition_test();
    let (observer, scope) = (shared(), ScopeHolder::new());
    let (o, s) = (observer.clone(), scope.clone());
    test.compose(move || Composition(o.clone(), s.clone()));
    test.validate(|v| v.linear(|v| v.text("Some text")));
    assert_eq!(observer.0.get(), 1);
    scope.invalidate();
    test.expect_no_changes();
    test.revalidate();
    assert_eq!(observer.0.get(), 1, "notified once");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberObserver_Remember_SingleNotification
#[test]
fn an_observer_remembered_twice_hears_two_on_remembered() {
    #[composable]
    fn Composition(observer: Shared, value: MutableState<i32>) {
        Linear!({
            let observer = observer.clone();
            remember_observer(move || observer);
            Text(&format!("Some text {}", value.get()));
        });
        Linear!({
            let observer = observer.clone();
            remember_observer(move || observer);
            Text(&format!("Some other text {}", value.get()));
        });
    }
    let mut test = composition_test();
    let observer = shared();
    let value = mutable_state_of(0);
    let (o, v) = (observer.clone(), value.handle());
    test.compose(move || Composition(o.clone(), v));
    assert_eq!(observer.0.get(), 2);
    value.update(|value| value + 1);
    test.expect_changes();
    test.validate(move |e| {
        e.linear(|e| e.text(&format!("Some text {}", v.get())));
        e.linear(|e| e.text(&format!("Some other text {}", v.get())));
    });
    assert_eq!(observer.0.get(), 2, "notified twice only");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberObserver_Forget_Simple
#[test]
fn a_removed_observer_hears_on_forgotten() {
    #[composable]
    fn Composition(include: bool, observer: Shared) {
        Linear!({
            if include {
                Linear!({
                    let observer = observer.clone();
                    remember_observer(move || observer);
                    Text("Some text");
                });
            }
        });
    }
    let mut test = composition_test();
    let observer = shared();
    let include = std::rc::Rc::new(std::cell::Cell::new(true));
    let scope = ScopeHolder::new();
    let (o, i, s) = (observer.clone(), include.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        Composition(i.get(), o.clone());
    });
    assert_eq!(observer.0.get(), 1);
    scope.invalidate();
    test.expect_no_changes();
    assert_eq!(observer.0.get(), 1);
    include.set(false);
    scope.invalidate();
    test.expect_changes();
    test.validate(|v| v.empty_linear());
    assert_eq!(observer.0.get(), 0, "forgotten");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRemember_Forget_ForgetOnRemember
#[test]
fn an_observer_moving_between_branches_is_forgotten_and_remembered() {
    #[composable]
    fn Composition(flags: [bool; 3], observer: Shared) {
        Linear!({
            for (index, name) in ["a", "b", "c"].into_iter().enumerate() {
                if flags[index] {
                    key(index, || {
                        Linear!({
                            let observer = observer.clone();
                            remember_observer(move || observer);
                            Text(name);
                        });
                    });
                }
            }
        });
    }
    let mut test = composition_test();
    let observer = shared();
    let flags = std::rc::Rc::new(std::cell::Cell::new([true, false, false]));
    let scope = ScopeHolder::new();
    let (o, f, s) = (observer.clone(), flags.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        Composition(f.get(), o.clone());
    });
    assert_eq!(observer.0.get(), 1);
    scope.invalidate();
    test.expect_no_changes();
    assert_eq!(observer.0.get(), 1);
    for next in [[false, true, false], [false, false, true], [true, false, false]] {
        flags.set(next);
        scope.invalidate();
        test.expect_changes();
        let shown = flags.clone();
        test.validate(move |v| {
            v.linear(|v| {
                for (index, name) in ["a", "b", "c"].into_iter().enumerate() {
                    if shown.get()[index] {
                        v.linear(|v| v.text(name));
                    }
                }
            })
        });
        assert_eq!(observer.0.get(), 1, "a forget and a remember");
    }
    flags.set([false; 3]);
    scope.invalidate();
    test.expect_changes();
    assert_eq!(observer.0.get(), 0, "a leave");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRemember_Forget_ForgetOnReplace
#[test]
fn a_keyed_observer_replaced_is_forgotten() {
    #[composable]
    fn Composition(observer: Option<Shared>) {
        Linear!({
            key(1, || {
                Linear!({
                    if let Some(observer) = observer.clone() {
                        remember_observer_keyed(observer.clone(), move || observer);
                    } else {
                        remember_keyed((), || ());
                    }
                    Text("Some value");
                });
            });
        });
    }
    let mut test = composition_test();
    let (first, second) = (shared(), shared());
    let current = var(Some(first.clone()));
    let scope = ScopeHolder::new();
    let (c, s) = (current.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        Composition(c.borrow().clone());
    });
    assert_eq!((first.0.get(), second.0.get()), (1, 0));
    *current.borrow_mut() = Some(second.clone());
    scope.invalidate();
    test.expect_changes();
    assert_eq!((first.0.get(), second.0.get()), (0, 1));
    *current.borrow_mut() = None;
    scope.invalidate();
    test.expect_changes();
    test.validate(|v| v.linear(|v| v.linear(|v| v.text("Some value"))));
    assert_eq!((first.0.get(), second.0.get()), (0, 0));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRemember_forget_forgetAfterPredecessorReplaced
#[test]
fn an_observer_after_a_replaced_key_group_stays() {
    #[composable]
    fn Content(key_value: i32, include: bool, observer: Shared) {
        if include {
            Linear!({
                key(key_value, || Linear!({ Text("Some value") }));
                let observer = observer.clone();
                remember_observer(move || observer);
            });
        }
    }
    let mut test = composition_test();
    let observer = shared();
    let (key_value, include) =
        (std::rc::Rc::new(std::cell::Cell::new(0)), std::rc::Rc::new(std::cell::Cell::new(true)));
    let scope = ScopeHolder::new();
    let (o, k, i, s) = (observer.clone(), key_value.clone(), include.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        Content(k.get(), i.get(), o.clone());
    });
    assert_eq!(observer.0.get(), 1);
    key_value.set(1);
    scope.invalidate();
    test.expect_changes();
    test.validate(|v| v.linear(|v| v.linear(|v| v.text("Some value"))));
    assert_eq!(observer.0.get(), 1, "still remembered");
    include.set(false);
    scope.invalidate();
    test.expect_changes();
    test.validate(|_| {});
    assert_eq!(observer.0.get(), 0, "left");
}

#[composable]
fn NestedTree(orders: Orders) {
    orders.remember("L0B");
    Linear!({
        orders.remember("L1B");
        Linear!({
            orders.remember("L2B");
            Linear!({
                orders.remember("L3B");
                Linear!({ orders.remember("Leaf") });
                orders.remember("L3A");
            });
            orders.remember("L2A");
        });
        orders.remember("L1A");
    });
    orders.remember("L0A");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRemember_RememberForgetNestedOrder
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRemember_RememberForgetNestedOrder_Inline
#[test]
fn nested_observers_are_forgotten_in_the_reverse_of_their_order() {
    #[composable]
    fn Content(include_tree: MutableState<bool>, orders: Orders) {
        if include_tree.get() {
            NestedTree(orders);
        }
    }
    let mut test = composition_test();
    let include_tree = mutable_state_of(true);
    let orders = Orders::new();
    let (i, o) = (include_tree.handle(), orders.clone());
    test.compose(move || Content(i, o.clone()));
    assert!(orders.all_counts(1), "all entered");
    include_tree.set(false);
    test.expect_changes();
    assert!(orders.all_counts(0), "all left");
    assert_eq!(
        orders.remembered(),
        ["L0B", "L1B", "L2B", "L3B", "Leaf", "L3A", "L2A", "L1A", "L0A"]
    );
    assert_eq!(
        orders.forgotten(),
        ["L0A", "L1A", "L2A", "L3A", "Leaf", "L3B", "L2B", "L1B", "L0B"]
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRemember_RememberForgetNestedOrder_Incremental
#[test]
fn observers_entering_level_by_level_are_forgotten_in_table_order() {
    #[composable]
    fn Tree(level: MutableState<i32>, orders: Orders) {
        orders.remember("L0B");
        Linear!({
            if level.get() >= 1 {
                orders.remember("L1B");
                if level.get() >= 2 {
                    Linear!({
                        orders.remember("L2B");
                        if level.get() >= 3 {
                            Linear!({
                                orders.remember("L3B");
                                if level.get() >= 4 {
                                    Linear!({ orders.remember("Leaf") });
                                }
                                orders.remember("L3A");
                            });
                        }
                        orders.remember("L2A");
                    });
                }
                orders.remember("L1A");
            }
        });
        orders.remember("L0A");
    }
    #[composable]
    fn Content(include_tree: MutableState<bool>, level: MutableState<i32>, orders: Orders) {
        if include_tree.get() {
            Tree(level, orders);
        }
    }
    let mut test = composition_test();
    let include_tree = mutable_state_of(true);
    let level = mutable_state_of(0);
    let orders = Orders::new();
    let (i, l, o) = (include_tree.handle(), level.handle(), orders.clone());
    test.compose(move || Content(i, l, o.clone()));
    while level.get() < 4 {
        level.update(|level| level + 1);
        test.expect_changes();
    }
    assert!(orders.all_counts(1), "all entered");
    include_tree.set(false);
    test.expect_changes();
    assert!(orders.all_counts(0), "all left");
    assert_eq!(
        orders.remembered(),
        ["L0B", "L0A", "L1B", "L1A", "L2B", "L2A", "L3B", "L3A", "Leaf"]
    );
    // Forgotten as if they had come all at once.
    assert_eq!(
        orders.forgotten(),
        ["L0A", "L1A", "L2A", "L3A", "Leaf", "L3B", "L2B", "L1B", "L0B"]
    );
}

/// The tree of `testRemember_RememberForgetOrder`: `user` remembers an observer named so.
#[composable(non_restartable)]
fn letter_tree(user: std::rc::Rc<dyn Fn(&'static str)>, j_inside: bool) {
    Linear!({
        user("A");
        Linear!({
            user("B");
            Linear!({
                user("C");
                user("D");
            });
            user("E");
            user("F");
            Linear!({
                user("G");
                user("H");
                Linear!({ user("I") });
                if j_inside {
                    user("J");
                }
            });
            user(if j_inside { "K" } else { "J" });
        });
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRemember_RememberForgetOrder
#[test]
fn observers_of_a_tree_of_composables_leave_in_reverse() {
    #[composable]
    fn RememberUser(name: String, orders: Orders) {
        Linear!({
            let (orders, tracked) = (orders.clone(), name.clone());
            remember_observer_keyed(name.clone(), move || orders.track(&tracked, 0));
            Text(&name);
        });
    }
    #[composable]
    fn Composition(include_tree: bool, orders: Orders) {
        Linear!({
            if include_tree {
                let orders = orders.clone();
                letter_tree(
                    std::rc::Rc::new(move |name| RememberUser(name.into(), orders.clone())),
                    false,
                );
            }
        });
    }
    let mut test = composition_test();
    let value = mutable_state_of(true);
    let orders = Orders::new();
    let (v, o) = (value.handle(), orders.clone());
    test.compose(move || Composition(v.get(), o.clone()));
    assert!(orders.all_counts(1));
    value.set(false);
    test.expect_changes();
    assert!(orders.all_counts(0));
    assert_eq!(orders.remembered(), ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J"]);
    assert_eq!(orders.forgotten(), ["J", "I", "H", "G", "F", "E", "D", "C", "B", "A"]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRemember_RememberForgetOrder_NonRestartable
#[test]
fn observers_of_non_restartable_composables_leave_in_reverse() {
    #[composable(non_restartable)]
    fn RememberUser(name: &str, orders: &Orders) {
        let (orders, name) = (orders.clone(), name.to_string());
        remember_observer_keyed(name.clone(), move || orders.track(&name, 0));
    }
    #[composable]
    fn Composition(include_tree: bool, orders: Orders) {
        Linear!({
            if include_tree {
                let orders = orders.clone();
                letter_tree(std::rc::Rc::new(move |name| RememberUser(name, &orders)), true);
            }
        });
    }
    let mut test = composition_test();
    let value = mutable_state_of(true);
    let orders = Orders::new();
    let (v, o) = (value.handle(), orders.clone());
    test.compose(move || Composition(v.get(), o.clone()));
    assert!(orders.all_counts(1));
    value.set(false);
    test.expect_changes();
    assert!(orders.all_counts(0));
    assert_eq!(orders.remembered(), ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K"]);
    assert_eq!(orders.forgotten(), ["K", "J", "I", "H", "G", "F", "E", "D", "C", "B", "A"]);
}

#[composable(non_restartable)]
fn DataUser(name: &str, data: i32, orders: &Orders) {
    let (orders, name) = (orders.clone(), name.to_string());
    remember_observer_keyed((name.clone(), data), move || orders.track(&name, data));
}

fn names_and_data_forgotten(orders: &Orders) -> String {
    let mut states: Vec<_> =
        orders.0.borrow().iter().filter(|state| state.forget_order.get() >= 0).cloned().collect();
    states.sort_by_key(|state| state.forget_order.get());
    states
        .iter()
        .map(|state| format!("{}:{}", state.name, state.data))
        .collect::<Vec<_>>()
        .join(", ")
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberObserver_RememberForgetOrder_ReplaceOnRecompose
#[test]
fn a_replaced_observer_is_forgotten_before_the_tree_leaves() {
    #[composable]
    fn Tree(changing: MutableState<i32>, orders: Orders) {
        Linear!({
            DataUser("A", changing.get(), &orders);
            DataUser("B", 10, &orders);
        });
    }
    #[composable]
    fn Composition(include_tree: MutableState<bool>, changing: MutableState<i32>, orders: Orders) {
        Linear!({
            if include_tree.get() {
                Tree(changing, orders.clone());
            }
        });
    }
    let mut test = composition_test();
    let (changing, include_tree) = (mutable_state_of(0), mutable_state_of(true));
    let orders = Orders::new();
    let (c, i, o) = (changing.handle(), include_tree.handle(), orders.clone());
    test.compose(move || Composition(i, c, o.clone()));
    changing.update(|changing| changing + 1);
    test.advance();
    include_tree.set(false);
    test.advance();
    assert_eq!(names_and_data_forgotten(&orders), "A:0, B:10, A:1");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberObserver_RememberForgetOrder_RelativeOrder
#[test]
fn replaced_and_removed_observers_are_forgotten_in_table_order() {
    #[composable(non_restartable)]
    fn Children(orders: &Orders) {
        DataUser("A2", 10, orders);
        DataUser("B2", 10, orders);
    }
    #[composable(non_restartable)]
    fn NoChildren() {}
    #[composable]
    fn Composition(
        changing: MutableState<i32>,
        include_children: MutableState<bool>,
        orders: Orders,
    ) {
        Linear!({
            DataUser("A1", changing.get(), &orders);
            if include_children.get() {
                Children(&orders);
            } else {
                NoChildren();
            }
            DataUser("B1", changing.get(), &orders);
        });
    }
    let mut test = composition_test();
    let (changing, include_children) = (mutable_state_of(0), mutable_state_of(true));
    let orders = Orders::new();
    let (c, i, o) = (changing.handle(), include_children.handle(), orders.clone());
    test.compose(move || Composition(c, i, o.clone()));
    changing.update(|changing| changing + 1);
    include_children.set(false);
    test.advance();
    assert_eq!(names_and_data_forgotten(&orders), "B1:0, B2:10, A2:10, A1:0");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRemember_RememberForgetOrder_keyChange
#[test]
fn observers_replaced_by_their_keys_are_forgotten_as_they_go() {
    #[composable]
    fn Test(a: MutableState<i32>, b: MutableState<i32>, orders: Orders) {
        let o = orders.clone();
        let name = format!("A{}", a.get());
        remember_observer_keyed(a.get(), move || o.track(&name, 0));
        let o = orders.clone();
        let name = format!("B{}", b.get());
        remember_observer_keyed(b.get(), move || o.track(&name, 0));
    }
    #[composable]
    fn Content(
        include: MutableState<bool>,
        a: MutableState<i32>,
        b: MutableState<i32>,
        orders: Orders,
    ) {
        if include.get() {
            Test(a, b, orders);
        }
    }
    let mut test = composition_test();
    let (a, b, include) = (mutable_state_of(0), mutable_state_of(0), mutable_state_of(true));
    let orders = Orders::new();
    let (ah, bh, ih, o) = (a.handle(), b.handle(), include.handle(), orders.clone());
    test.compose(move || Content(ih, ah, bh, o.clone()));
    a.update(|a| a + 1);
    test.advance();
    b.update(|b| b + 1);
    test.advance();
    a.update(|a| a + 1);
    test.advance();
    include.set(false);
    test.advance();
    assert!(orders.all_counts(0), "all left");
    assert_eq!(orders.remembered(), ["A0", "B0", "A1", "B1", "A2"]);
    assert_eq!(orders.forgotten(), ["A0", "B0", "A1", "B1", "A2"]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberObserver_Abandon_Simple
#[test]
fn an_observer_remembered_by_a_failed_pass_is_abandoned() {
    let orders = Orders::new();
    let o = orders.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let mut test = composition_test();
        test.compose(move || {
            o.remember("observed");
            panic!("Throw");
        });
    }));
    assert!(result.is_err());
    let observed = orders.0.borrow()[0].clone();
    assert!(observed.abandoned.get(), "abandoned");
    assert_eq!(observed.remember_order.get(), -1, "never remembered");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberObserver_Abandon_Recompose_GapBuffer
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRememberObserver_Abandon_Recompose_LinkTable
#[test]
fn an_observer_remembered_by_a_failed_recomposition_is_abandoned() {
    #[composable]
    fn Content(remember_object: MutableState<bool>, orders: Orders) {
        if remember_object.get() {
            orders.remember("observed");
            panic!("Throw");
        }
    }
    let orders = Orders::new();
    let remember_object = mutable_state_of(false);
    let (r, o) = (remember_object.handle(), orders.clone());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let mut test = composition_test();
        test.compose(move || Content(r, o.clone()));
        r.set(true);
        test.advance_ignoring_pending_work();
    }));
    assert!(result.is_err());
    let observed = orders.0.borrow()[0].clone();
    assert!(observed.abandoned.get(), "abandoned");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testRemember_OrderingWithAlternatingEffects
#[test]
fn observers_between_side_effects_keep_their_order() {
    #[composable]
    fn Content(show_content: MutableState<bool>, run_effects: MutableState<bool>, order: Log) {
        if show_content.get() {
            for name in ["1", "2", "3", "4"] {
                let order = order.clone();
                remember_observer(move || Logged(name, order));
                if run_effects.get() {
                    SideEffect(|| {});
                }
            }
        }
    }
    struct Logged(&'static str, Log);
    impl RememberObserver for Logged {
        fn on_remembered(&self) {
            self.1.push(format!("R[{}]", self.0));
        }
        fn on_forgotten(&self) {
            self.1.push(format!("F[{}]", self.0));
        }
        fn on_abandoned(&self) {
            self.1.push(format!("A[{}]", self.0));
        }
    }
    let mut test = composition_test();
    let (show_content, run_effects) = (mutable_state_of(true), mutable_state_of(false));
    let order = Log::new();
    let (s, r, o) = (show_content.handle(), run_effects.handle(), order.clone());
    test.compose(move || Content(s, r, o.clone()));
    show_content.set(false);
    test.advance();
    assert_eq!(order.joined(), "R[1], R[2], R[3], R[4], F[4], F[3], F[2], F[1]");
}

// ---- Keys, groups and observation scopes ---------------------------------------------------

#[composable]
fn Container(content: impl Fn() + 'static) {
    content();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testCompoundKeyHashCodeStaysTheSameAfterRecompositions
#[test]
fn the_composite_key_hash_stays_the_same_across_recompositions() {
    #[composable]
    fn Test(outer_keys: Log, inner_keys: Log, outer: ScopeHolder, inner: ScopeHolder) {
        outer.capture();
        outer_keys.push(current_composite_key_hash().to_string());
        let (inner_keys, inner) = (inner_keys.clone(), inner.clone());
        Container!({
            let (inner_keys, inner) = (inner_keys.clone(), inner.clone());
            Linear!({
                inner.capture();
                inner_keys.push(current_composite_key_hash().to_string());
            });
        });
        assert_eq!(
            outer_keys.entries().last().cloned(),
            Some(current_composite_key_hash().to_string()),
            "the key is rolled back after the content"
        );
    }
    let mut test = composition_test();
    let (outer_keys, inner_keys) = (Log::new(), Log::new());
    let (outer, inner) = (ScopeHolder::new(), ScopeHolder::new());
    let (ok, ik, o, i) = (outer_keys.clone(), inner_keys.clone(), outer.clone(), inner.clone());
    test.compose(move || Test(ok.clone(), ik.clone(), o.clone(), i.clone()));
    let outer_size = outer_keys.entries().len();
    outer.invalidate();
    test.expect_no_changes();
    assert_ne!(outer_keys.entries().len(), outer_size);
    let inner_size = inner_keys.entries().len();
    inner.invalidate();
    test.expect_no_changes();
    assert_ne!(inner_keys.entries().len(), inner_size);
    let (outer_keys, inner_keys) = (outer_keys.entries(), inner_keys.entries());
    assert_ne!(inner_keys[0], outer_keys[0]);
    assert!(inner_keys.iter().all(|key| *key == inner_keys[0]));
    assert!(outer_keys.iter().all(|key| *key == outer_keys[0]));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testSwappingGroups
#[test]
fn keyed_groups_without_nodes_swap() {
    #[composable]
    fn NoNodes() {}
    let mut test = composition_test();
    let items = var(vec![0, 1, 2, 3, 4]);
    let scope = ScopeHolder::new();
    let (i, s) = (items.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        for item in i.borrow().clone() {
            key(item, NoNodes);
        }
    });
    items.borrow_mut().swap(2, 3);
    scope.invalidate();
    test.expect_changes();
    test.verify_consistent();
}

/// Invalidates every scope added since the last call, as `invalidateComposition()`.
#[derive(Clone, Default)]
struct Invalidates(std::rc::Rc<std::cell::RefCell<Vec<RecomposeScope>>>);

impl ComposeArg for Invalidates {
    fn same(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Invalidates {
    fn add(&self) {
        self.0.borrow_mut().push(current_recompose_scope());
    }

    fn invalidate(&self) {
        for scope in std::mem::take(&mut *self.0.borrow_mut()) {
            scope.invalidate();
        }
    }
}

type Items = Var<Vec<(i32, Vec<i32>)>>;

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testInsertOnMultipleLevels
#[test]
fn insertions_on_several_levels_in_one_pass() {
    #[composable]
    fn Numbers(numbers: std::rc::Rc<Vec<i32>>, invalidates: Invalidates) {
        Linear!({
            Linear!({
                invalidates.add();
                for number in numbers.iter() {
                    Text(&number.to_string());
                }
            });
        });
    }
    #[composable]
    fn Item(number: i32, numbers: std::rc::Rc<Vec<i32>>, invalidates: Invalidates) {
        Linear!({
            invalidates.add();
            Text(&number.to_string());
            Numbers(numbers.clone(), invalidates.clone());
        });
    }
    #[composable]
    fn Test(items: Items, invalidates: Invalidates) {
        invalidates.add();
        Linear!({
            invalidates.add();
            for (number, numbers) in items.borrow().iter() {
                Item(*number, std::rc::Rc::new(numbers.clone()), invalidates.clone());
            }
        });
    }
    let mut test = composition_test();
    let items: Items = var(vec![(1, vec![0, 1, 2, 3, 4]), (3, vec![0, 1, 2, 3, 4])]);
    let invalidates = Invalidates::default();
    let (i, inv) = (items.clone(), invalidates.clone());
    test.compose(move || Test(i.clone(), inv.clone()));
    let shown = items.clone();
    test.validate(move |v| {
        v.linear(|v| {
            for (number, numbers) in shown.borrow().iter() {
                v.linear(|v| {
                    v.text(&number.to_string());
                    v.linear(|v| v.linear(|v| numbers.iter().for_each(|n| v.text(&n.to_string()))));
                });
            }
        })
    });
    items.borrow_mut()[0].1.insert(2, 100);
    items.borrow_mut()[1].1.insert(3, 200);
    items.borrow_mut().insert(1, (2, vec![0, 1, 2]));
    invalidates.invalidate();
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testInsertingAfterSkipping
#[test]
fn an_insertion_after_skipped_groups() {
    // The items are state: Kotlin re-runs the container's lambda with its parent, compose-rs
    // memoizes it on its captures (D710).
    type Items = MutableState<Vec<(i32, Vec<i32>)>>;
    #[composable]
    fn Test(items: Items, invalidates: Invalidates) {
        invalidates.add();
        Linear!({
            for (item, numbers) in items.get() {
                Text(&item.to_string());
                let invalidates = invalidates.clone();
                let numbers = std::rc::Rc::new(numbers);
                Linear!({
                    invalidates.add();
                    for number in numbers.iter() {
                        Text(&number.to_string());
                    }
                });
            }
        });
    }
    let mut test = composition_test();
    let items = mutable_state_of_with(vec![(1, vec![0, 1, 2, 3, 4])], NeverEqualPolicy);
    let invalidates = Invalidates::default();
    let (i, inv) = (items.handle(), invalidates.clone());
    test.compose(move || Test(i, inv.clone()));
    test.validate(move |v| {
        v.linear(|v| {
            for (item, numbers) in i.get() {
                v.text(&item.to_string());
                v.linear(|v| numbers.iter().for_each(|n| v.text(&n.to_string())));
            }
        })
    });
    items.edit(|items| items.push((2, vec![3, 4, 5, 6])));
    invalidates.invalidate();
    test.expect_changes();
    test.revalidate();
}

/// `(include_even, include_odd)`, as the two Kotlin `var`s of the even-odd tests.
type EvenOdd = std::rc::Rc<std::cell::Cell<(bool, bool)>>;

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, evenOddRecomposeGroup
#[test]
fn keyed_texts_come_and_go_by_parity() {
    #[composable]
    fn EmitText(flags: EvenOdd, invalidates: Invalidates) {
        invalidates.add();
        let (even, odd) = flags.get();
        if odd {
            key(1, || Text("odd 1"));
        }
        if even {
            key(2, || Text("even 2"));
        }
        if odd {
            key(3, || Text("odd 3"));
        }
        if even {
            key(4, || Text("even 4"));
        }
    }
    #[composable]
    fn Test(flags: EvenOdd, invalidates: Invalidates) {
        Linear!({
            Wrap!({ EmitText(flags.clone(), invalidates.clone()) });
            EmitText(flags.clone(), invalidates.clone());
            Wrap!({ EmitText(flags.clone(), invalidates.clone()) });
            EmitText(flags.clone(), invalidates.clone());
        });
    }
    let mut test = composition_test();
    let flags: EvenOdd = std::rc::Rc::new(std::cell::Cell::new((true, true)));
    let invalidates = Invalidates::default();
    let (f, i) = (flags.clone(), invalidates.clone());
    test.compose(move || Test(f.clone(), i.clone()));
    let shown = flags.clone();
    test.validate(move |v| {
        v.linear(|v| {
            for _ in 0..4 {
                let (even, odd) = shown.get();
                for (number, is_odd) in [(1, true), (2, false), (3, true), (4, false)] {
                    if (is_odd && odd) || (!is_odd && even) {
                        v.text(&format!("{} {number}", if is_odd { "odd" } else { "even" }));
                    }
                }
            }
        })
    });
    for next in [(false, true), (true, false), (false, false), (true, false), (true, true)] {
        flags.set(next);
        invalidates.invalidate();
        test.expect_changes();
        test.revalidate();
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testMovingSlotsButNotNodes
#[test]
fn keyed_groups_move_their_slots_without_nodes() {
    #[composable]
    fn Content(order: MutableState<Vec<i32>>, include_text: MutableState<Vec<i32>>) {
        for i in order.get() {
            key(i, || {
                if include_text.with(|included| included.contains(&i)) {
                    Text(&format!("Text for {i}"));
                }
            });
        }
        Text("End");
    }
    let mut test = composition_test();
    let order = mutable_state_list_from([1, 2, 3, 4, 5]);
    let include_text = mutable_state_list_from([4]);
    let (o, i) = (order.handle(), include_text.handle());
    test.compose(move || Content(o, i));
    test.validate(move |v| {
        for item in o.get() {
            if i.with(|included| included.contains(&item)) {
                v.text(&format!("Text for {item}"));
            }
        }
        v.text("End");
    });
    order.edit(|order| order.swap(2, 4));
    include_text.edit(|included| {
        included.retain(|&item| item != 4);
        included.push(3);
    });
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, evenOddWithMovement
#[test]
fn keyed_groups_move_while_their_content_changes() {
    #[composable]
    fn EmitText(all: bool, order: Var<Vec<i32>>, flags: EvenOdd, invalidates: Invalidates) {
        invalidates.add();
        let (even, odd) = flags.get();
        for i in order.borrow().clone() {
            if i % 2 == 1 && (all || odd) {
                key(i, || Text(&format!("odd {i}")));
            }
            if i % 2 == 0 && (all || even) {
                key(i, || Text(&format!("even {i}")));
            }
        }
    }
    #[composable]
    fn Test(order: Var<Vec<i32>>, flags: EvenOdd, invalidates: Invalidates) {
        Linear!({
            invalidates.add();
            for i in order.borrow().clone() {
                key(i, || {
                    Text(&format!("group {i}"));
                    let (even, odd) = flags.get();
                    if i == 2 || (even && odd) {
                        Text("including everything");
                    } else {
                        if even {
                            Text("including evens");
                        }
                        if odd {
                            Text("including odds");
                        }
                    }
                    EmitText(i == 2, order.clone(), flags.clone(), invalidates.clone());
                });
            }
            EmitText(false, order.clone(), flags.clone(), invalidates.clone());
        });
    }
    fn expect_emit(v: &mut Expected, all: bool, order: &[i32], (even, odd): (bool, bool)) {
        for &i in order {
            if i % 2 == 1 && (odd || all) {
                v.text(&format!("odd {i}"));
            }
            if i % 2 == 0 && (even || all) {
                v.text(&format!("even {i}"));
            }
        }
    }
    let mut test = composition_test();
    let order = var(vec![1, 2, 3, 4]);
    let flags: EvenOdd = std::rc::Rc::new(std::cell::Cell::new((true, true)));
    let invalidates = Invalidates::default();
    let (o, f, i) = (order.clone(), flags.clone(), invalidates.clone());
    test.compose(move || Test(o.clone(), f.clone(), i.clone()));
    let (shown_order, shown_flags) = (order.clone(), flags.clone());
    test.validate(move |v| {
        v.linear(|v| {
            let order = shown_order.borrow().clone();
            let (even, odd) = shown_flags.get();
            for &i in &order {
                v.text(&format!("group {i}"));
                if i == 2 || (even && odd) {
                    v.text("including everything");
                } else {
                    if even {
                        v.text("including evens");
                    }
                    if odd {
                        v.text("including odds");
                    }
                }
                expect_emit(v, i == 2, &order, (even, odd));
            }
            expect_emit(v, false, &order, (even, odd));
        })
    });
    for (next_order, next_flags) in [
        (vec![1, 2, 4, 3], (false, true)),
        (vec![1, 4, 2, 3], (true, false)),
        (vec![3, 4, 2, 1], (false, false)),
        (vec![4, 3, 2, 1], (true, false)),
        (vec![1, 2, 3, 4], (true, true)),
    ] {
        *order.borrow_mut() = next_order;
        flags.set(next_flags);
        invalidates.invalidate();
        test.expect_changes();
        test.revalidate();
    }
}

type States = std::rc::Rc<std::cell::RefCell<Vec<OwnedState<i32>>>>;

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testObservationScopes
#[test]
fn only_the_states_of_the_latest_run_invalidate() {
    // Compose's unremembered states live while the test references them; here the test keeps
    // their owners.
    #[composable]
    fn Test(states: States, iterations: Counter) {
        let s1 = mutable_state_of(iterations.increment() as i32 - 1);
        Text(&format!("s1 {}", s1.get()));
        states.borrow_mut().push(s1);
        let s2 = mutable_state_of(iterations.increment() as i32 - 1);
        Text(&format!("s2 {}", s2.get()));
        states.borrow_mut().push(s2);
    }
    let mut test = composition_test();
    let states: States = std::rc::Rc::default();
    let iterations = Counter::new();
    let (s, i) = (states.clone(), iterations.clone());
    test.compose(move || Test(s.clone(), i.clone()));
    let bump = |first: bool| {
        let states = states.borrow();
        let state = if first { states.first() } else { states.last() };
        state.expect("a state").update(|value| value + 1);
    };
    for _ in 0..10 {
        bump(false);
        test.expect_changes();
    }
    bump(true);
    test.expect_no_changes();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testObservationScopes_IndirectInvalidate
#[test]
fn a_state_of_an_earlier_run_invalidates_nothing() {
    #[composable]
    fn Indirect(iteration: i32, states: States) {
        let state = mutable_state_of(iteration * 1000 + 7);
        Text(&format!("{iteration} state = {}", state.get()));
        states.borrow_mut().push(state);
    }
    #[composable]
    fn ComposeIndirect(iteration: State<i32>, states: States) {
        Text(&format!("Iteration {}", iteration.get()));
        Indirect(iteration.get(), states);
    }
    let mut test = composition_test();
    let states: States = std::rc::Rc::default();
    let iteration = mutable_state_of(0);
    let (s, i) = (states.clone(), iteration.as_state());
    test.compose(move || ComposeIndirect(i, s.clone()));
    for _ in 0..10 {
        iteration.update(|iteration| iteration + 1);
        test.expect_changes();
    }
    states.borrow().first().expect("a state").update(|value| value + 1);
    test.expect_no_changes();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testApplierBeginEndCallbacks
#[test]
fn the_applier_hears_a_pass_begin_and_end_before_its_effects() {
    // Compose calls `onBeginChanges` only once composition is over, when it applies its
    // recorded changes; compose-rs applies changes as the pass composes (D31), so the applier
    // hears the pass begin before composition, and end before observers and effects.
    let mut test = composition_test();
    let checks = Log::new();
    let c = checks.clone();
    test.compose(move || {
        let counts = compose_runtime::platform::current()
            .with_applier(|views: &mut ViewApplier| (views.begin_changes, views.end_changes));
        assert_eq!(counts, (1, 0), "begun, not ended, during composition");
        c.push("composition");
        let c2 = c.clone();
        SideEffect(move || c2.push("SideEffect"));
        struct Check(Log);
        impl RememberObserver for Check {
            fn on_remembered(&self) {
                self.0.push("RememberObserver");
            }
        }
        let c3 = c.clone();
        remember_observer(move || Check(c3));
    });
    assert_eq!(test.with_views(|views| (views.begin_changes, views.end_changes)), (1, 1));
    assert_eq!(checks.entries(), ["composition", "RememberObserver", "SideEffect"]);
}
