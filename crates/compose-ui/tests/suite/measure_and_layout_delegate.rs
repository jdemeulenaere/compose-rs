//! Compose's `MeasureAndLayoutDelegateTest`, rewritten on a `UiHost`: its `LayoutNode`s built by
//! hand are layouts here, each with the test helpers' `SmartMeasurePolicy` (counting its
//! measurements and placement blocks, a size, wrapping its children or not, measuring them in its
//! measurement or in its block, callbacks run while it measures and places), and the delegate's
//! `requestRemeasure` and `requestRelayout` are the node's `invalidate_measurement` and
//! `invalidate_placement` (through a handle its modifier node keeps). `measureAndLayout` is a
//! frame; what it returns, whether the root's size changed, is read from the root's size.
//!
//! n/a: the tests of `LayoutNode`'s own flags (`requiresMeasureWhenJustCreated`,
//! `childRemeasureRequest_remeasureRequired`, `childRelayoutRequest_childRelayoutRequired`,
//! `rootNodeIsPlacedWhenAttached`): compose-rs has no `measurePending` or `layoutPending` to ask.

use crate::support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::{MutableState, callback, mutable_state_of};
use compose_ui::platform::UiHost;
use compose_ui::*;
use support::*;

/// A callback run while measuring or placing, and whether it's run only once.
type During = RefCell<Option<(Rc<dyn Fn()>, bool)>>;

/// A layout node as the helpers' `node { … }`: its policy's settings and counts, its children.
#[derive(Default)]
struct N {
    measures: Cell<u32>,
    layouts: Cell<u32>,
    wrap_children: Cell<bool>,
    /// The children's maximum size, if set (`size`); else the constraints'.
    size: Cell<Option<i32>>,
    /// `measureInLayoutBlock`.
    in_layout_block: Cell<bool>,
    /// `doNotMeasure`.
    no_measure: Cell<bool>,
    /// `shouldPlaceChildren` false.
    dont_place: Cell<bool>,
    pre_measure: During,
    pre_layout: During,
    /// Its size, as it last measured itself.
    measured: Cell<IntSize>,
    handle: Cell<Option<NodeHandle>>,
    /// Its modifier after the handle's, if set (`modifier = …`).
    modifier: RefCell<Option<Modifier>>,
    children: RefCell<Vec<Node>>,
}

type Node = Rc<N>;

/// `node { … }`.
fn node(build: impl FnOnce(&Node)) -> Node {
    let node = Rc::new(N::default());
    build(&node);
    node
}

fn leaf() -> Node {
    node(|_| {})
}

impl N {
    fn add(&self, child: Node) {
        self.children.borrow_mut().push(child);
    }

    fn first(&self) -> Node {
        self.children.borrow()[0].clone()
    }

    fn second(&self) -> Node {
        self.children.borrow()[1].clone()
    }

    fn handle(&self) -> NodeHandle {
        self.handle.get().expect("attached")
    }

    /// `delegate.requestRemeasure(node)`.
    fn request_remeasure(&self) {
        self.handle().invalidate_measurement();
    }

    /// `delegate.requestRelayout(node)`.
    fn request_relayout(&self) {
        self.handle().invalidate_placement();
    }

    /// `runDuringMeasure(once) { … }`.
    fn run_during_measure(&self, once: bool, block: impl Fn() + 'static) {
        *self.pre_measure.borrow_mut() = Some((Rc::new(block), once));
    }

    /// `runDuringLayout(once) { … }`.
    fn run_during_layout(&self, once: bool, block: impl Fn() + 'static) {
        *self.pre_layout.borrow_mut() = Some((Rc::new(block), once));
    }

    fn counts(&self) -> (u32, u32) {
        (self.measures.get(), self.layouts.get())
    }
}

fn bump(count: &Cell<u32>) {
    count.set(count.get() + 1);
}

fn run(during: &During) {
    let Some((block, once)) = during.borrow().clone() else { return };
    if once {
        *during.borrow_mut() = None;
    }
    block();
}

/// The helpers' `MeasureInMeasureBlock`, `MeasureInLayoutBlock` and `NoMeasure`, as the node says.
fn policy(n: Node) -> Rc<dyn MeasurePolicy> {
    measure_policy(move |scope, measurables, constraints| {
        bump(&n.measures);
        run(&n.pre_measure);
        let child = match n.size.get() {
            None => constraints,
            Some(size) => {
                Constraints::new(constraints.min_width, size, constraints.min_height, size)
            }
        };
        let n2 = n.clone();
        if n.no_measure.get() {
            let wrap = n.wrap_children.get();
            let width = (n.size.get()).unwrap_or(if wrap {
                constraints.min_width
            } else {
                constraints.max_width
            });
            let height = (n.size.get()).unwrap_or(if wrap {
                constraints.min_height
            } else {
                constraints.max_height
            });
            n.measured.set(IntSize::new(width, height));
            return scope.layout_with(width, height, move |_| {
                bump(&n2.layouts);
                run(&n2.pre_layout);
            });
        }
        if n.in_layout_block.get() {
            let measurables = measurables.to_vec();
            n.measured.set(IntSize::new(child.max_width, child.max_height));
            return scope.layout_with(child.max_width, child.max_height, move |placement| {
                run(&n2.pre_layout);
                bump(&n2.layouts);
                for &measurable in &measurables {
                    let placeable = placement.measure(measurable, child);
                    if !n2.dont_place.get() {
                        placement.place(&placeable, 0, 0);
                    }
                }
            });
        }
        let placeables: Vec<Placeable> =
            measurables.iter().map(|&measurable| scope.measure(measurable, child)).collect();
        let (width, height) = if n.wrap_children.get() {
            (placeables.iter())
                .fold((0, 0), |(w, h), placeable| (w.max(placeable.width), h.max(placeable.height)))
        } else {
            (child.max_width, child.max_height)
        };
        n.measured.set(IntSize::new(width, height));
        scope.layout_with(width, height, move |placement| {
            bump(&n2.layouts);
            run(&n2.pre_layout);
            if !n2.dont_place.get() {
                for placeable in &placeables {
                    placement.place(placeable, 0, 0);
                }
            }
        })
    })
}

/// Keeps its layout node's handle in the node, for the requests.
struct HandleElement(Node);

struct HandleNode(Node);

impl ModifierNodeElement for HandleElement {
    type Node = HandleNode;
    fn create(&self) -> HandleNode {
        HandleNode(self.0.clone())
    }
    fn update(&self, node: &mut HandleNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ModifierNode for HandleNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.0.handle.set(Some(context.handle()));
    }
}

fn emit(n: &Node) {
    let children = n.children.borrow().clone();
    let mut modifier = Modifier.then_custom(HandleElement(n.clone()));
    if let Some(more) = n.modifier.borrow().clone() {
        modifier = modifier.then(more);
    }
    Layout(
        modifier,
        policy(n.clone()),
        callback(move || {
            for child in &children {
                emit(child);
            }
        }),
    );
}

/// `createDelegate(root)`: the root in a host, measured in the helpers' root constraints (at most
/// 100 by 100) and laid out.
fn delegate(root: &Node) -> UiHost {
    delegate_in(root, None)
}

/// [`delegate`], the root constraints' maximum size read from `max` if given
/// (`updateRootConstraints`).
fn delegate_in(root: &Node, max: Option<MutableState<i32>>) -> UiHost {
    let root = root.clone();
    host(move || {
        let root = root.clone();
        Layout(
            Modifier,
            measure_policy(move |scope, measurables, _| {
                let max = max.map_or(100, |max| max.get());
                let placeable = scope.measure(measurables[0], Constraints::new(0, max, 0, max));
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            }),
            callback(move || emit(&root)),
        );
    })
}

/// `delegate.measureAndLayout()`: whether the root's size changed.
fn measure_and_layout(host: &mut UiHost, root: &Node) -> bool {
    let before = root.measured.get();
    host.frame();
    root.measured.get() != before
}

/// What `act` changed of `node`'s measurements and placements.
#[track_caller]
fn delta(node: &Node, act: impl FnOnce()) -> (u32, u32) {
    let (measures, layouts) = node.counts();
    act();
    (node.measures.get() - measures, node.layouts.get() - layouts)
}

const DIFFERENT_SIZE: i32 = 50;
const DIFFERENT_SIZE_2: i32 = 30;

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, measureNotRequiredAfterFirstMeasure,
// relayoutNotRequiredAfterFirstMeasure, measuredAndLaidOutAfterFirstMeasureAndLayout
#[test]
fn the_first_frame_measures_and_places_each_node_once() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    assert_eq!(root.counts(), (1, 1));
    assert_eq!(root.first().counts(), (1, 1));
    host.frame();
    assert_eq!(root.counts(), (1, 1), "nothing left to do");
    assert_eq!(root.first().counts(), (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childRemeasureRequest_childRemeasured,
// childRemeasureRequest_childRelaidOut
#[test]
fn a_child_asked_to_measure_again_is_measured_and_placed_again() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let changed = delta(&child, || {
        child.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childMeasuredInLayoutBlockRemeasureRequest_childRemeasured,
// childMeasuredInLayoutBlockRelayoutRequest_childRelaidOut
#[test]
fn a_child_measured_in_a_block_asked_to_measure_again_is_measured_and_placed_again() {
    let root = node(|it| {
        it.add(leaf());
        it.in_layout_block.set(true);
    });
    let mut host = delegate(&root);
    let child = root.first();
    let changed = delta(&child, || {
        child.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childRemeasureWithTheSameResult_parentNotRemeasured,
// childRemeasureWithTheSameResult_parentNotRelaidOut
#[test]
fn a_child_measured_again_to_the_same_size_leaves_its_parent() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let changed = delta(&root, || {
        root.first().request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, (0, 0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childRemeasureWithDifferentResult_parentRemeasured
#[test]
fn a_child_measured_again_to_another_size_measures_its_wrapping_parent_again() {
    let root = node(|it| {
        it.wrap_children.set(true);
        it.add(node(|child| child.size.set(Some(DIFFERENT_SIZE))));
    });
    let mut host = delegate(&root);
    let child = root.first();
    let changed = deltas(&[&root, &child], || {
        child.size.set(Some(DIFFERENT_SIZE_2));
        child.request_remeasure();
        assert!(measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(1, 1), (1, 1)]);
    assert_eq!(root.measured.get(), IntSize::new(DIFFERENT_SIZE_2, DIFFERENT_SIZE_2));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childRemeasureInLayoutBlockWithTheSameResult_parentNotRemeasured,
// childRemeasureInLayoutBlockWithTheSameResult_parentNotRelaidOut
#[test]
fn a_child_measured_in_a_block_again_to_the_same_size_leaves_its_parent() {
    let root = node(|it| {
        it.add(leaf());
        it.in_layout_block.set(true);
    });
    let mut host = delegate(&root);
    let changed = delta(&root, || {
        root.first().request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, (0, 0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childRemeasureInLayoutBlockWithDifferentResult_parentNotRemeasured,
// childRemeasureInLayoutBlockWithDifferentResult_parentRelaidOut
#[test]
fn a_child_measured_in_a_block_again_to_another_size_places_its_parent_again() {
    let root = node(|it| {
        it.add(leaf());
        it.in_layout_block.set(true);
    });
    let mut host = delegate(&root);
    let child = root.first();
    let changed = deltas(&[&root, &child], || {
        child.size.set(Some(DIFFERENT_SIZE));
        child.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 1), (1, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, rootRemeasureRequest_childNotAffected
#[test]
fn the_root_measured_again_leaves_its_child() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let mut child_changed = (0, 0);
    let root_changed = delta(&root, || {
        child_changed = delta(&child, || {
            root.request_remeasure();
            assert!(!measure_and_layout(&mut host, &root));
        })
    });
    assert_eq!(root_changed, (1, 1));
    assert_eq!(child_changed, (0, 0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, parentRemeasureRequest_childNotAffected
#[test]
fn a_parent_measured_again_leaves_its_child() {
    let root = node(|it| it.add(node(|it| it.add(leaf()))));
    let mut host = delegate(&root);
    let (parent, child) = (root.first(), root.first().first());
    let mut child_changed = (0, 0);
    let parent_changed = delta(&parent, || {
        child_changed = delta(&child, || {
            parent.request_remeasure();
            assert!(!measure_and_layout(&mut host, &root));
        })
    });
    assert_eq!(parent_changed, (1, 1));
    assert_eq!(child_changed, (0, 0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childRelayoutRequest_childRelaidOut,
// childRelayoutRequest_childNotRemeasured, childRelayoutRequest_parentNotRemeasured,
// childRelayoutRequest_parentNotRelaidOut
#[test]
fn a_child_asked_to_place_again_is_placed_again_alone() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let mut child_changed = (0, 0);
    let root_changed = delta(&root, || {
        child_changed = delta(&child, || {
            child.request_relayout();
            assert!(!measure_and_layout(&mut host, &root));
        })
    });
    assert_eq!(child_changed, (0, 1));
    assert_eq!(root_changed, (0, 0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childMeasuredInLayoutBlockRelayoutRequest_parentNotRemeasured,
// childMeasuredInLayoutBlockRelayoutRequest_parentNotRelaidOut
#[test]
fn a_child_measured_in_a_block_asked_to_place_again_leaves_its_parent() {
    let root = node(|it| {
        it.add(leaf());
        it.in_layout_block.set(true);
    });
    let mut host = delegate(&root);
    let child = root.first();
    let changed = deltas(&[&root, &child], || {
        child.request_relayout();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (0, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, rootRelayoutRequest_childNotAffected
#[test]
fn the_root_placed_again_leaves_its_child() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let mut child_changed = (0, 0);
    let root_changed = delta(&root, || {
        child_changed = delta(&child, || {
            root.request_relayout();
            assert!(!measure_and_layout(&mut host, &root));
        })
    });
    assert_eq!(root_changed, (0, 1));
    assert_eq!(child_changed, (0, 0));
}

/// `request` then `then` of the child, a frame: what changed of it.
fn requested_twice(request: impl Fn(&Node), then: impl Fn(&Node)) -> (u32, u32) {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    delta(&child, || {
        request(&child);
        then(&child);
        assert!(!measure_and_layout(&mut host, &root));
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childRemeasureRequestedTwice_childRemeasuredOnce,
// childRemeasureRequestedTwice_childRelaidOutOnce
#[test]
fn a_child_asked_twice_to_measure_again_is_measured_and_placed_once() {
    assert_eq!(requested_twice(|it| it.request_remeasure(), |it| it.request_remeasure()), (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childRemeasureAndRelayoutRequested_childRemeasured,
// childRemeasureAndRelayoutRequested_childRelaidOut
#[test]
fn a_child_asked_to_measure_then_place_again_is_measured_and_placed_once() {
    assert_eq!(requested_twice(|it| it.request_remeasure(), |it| it.request_relayout()), (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childRelayoutAndRemeasureRequested_childRemeasured,
// childRelayoutAndRemeasureRequested_childRelaidOut
#[test]
fn a_child_asked_to_place_then_measure_again_is_measured_and_placed_once() {
    assert_eq!(requested_twice(|it| it.request_relayout(), |it| it.request_remeasure()), (1, 1));
}

fn two_children() -> (Node, UiHost) {
    let root = node(|it| {
        it.add(leaf());
        it.add(leaf());
    });
    let host = delegate(&root);
    (root, host)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, firstChildRemeasureRequest_onlyFirstChildRemeasured
#[test]
fn the_first_child_asked_to_measure_again_is_the_only_one_measured() {
    let (root, mut host) = two_children();
    let (first, second) = (root.first(), root.second());
    let mut second_changed = (0, 0);
    let first_changed = delta(&first, || {
        second_changed = delta(&second, || {
            first.request_remeasure();
            assert!(!measure_and_layout(&mut host, &root));
        })
    });
    assert_eq!(first_changed, (1, 1));
    assert_eq!(second_changed, (0, 0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, firstChildRelayoutRequest_onlyFirstChildRelaid
#[test]
fn the_first_child_asked_to_place_again_is_the_only_one_placed() {
    let (root, mut host) = two_children();
    let (first, second) = (root.first(), root.second());
    let mut second_changed = (0, 0);
    let first_changed = delta(&first, || {
        second_changed = delta(&second, || {
            first.request_relayout();
            assert!(!measure_and_layout(&mut host, &root));
        })
    });
    assert_eq!(first_changed, (0, 1));
    assert_eq!(second_changed, (0, 0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, bothChildrenRemeasureRequest_bothRemeasured
#[test]
fn both_children_asked_to_measure_again_are_measured() {
    let (root, mut host) = two_children();
    let (first, second) = (root.first(), root.second());
    let mut second_changed = (0, 0);
    let first_changed = delta(&first, || {
        second_changed = delta(&second, || {
            first.request_remeasure();
            second.request_remeasure();
            assert!(!measure_and_layout(&mut host, &root));
        })
    });
    assert_eq!((first_changed, second_changed), ((1, 1), (1, 1)));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, bothChildrenRelayoutRequest_bothRelaidOut
#[test]
fn both_children_asked_to_place_again_are_placed() {
    let (root, mut host) = two_children();
    let (first, second) = (root.first(), root.second());
    let mut second_changed = (0, 0);
    let first_changed = delta(&first, || {
        second_changed = delta(&second, || {
            first.request_relayout();
            second.request_relayout();
            assert!(!measure_and_layout(&mut host, &root));
        })
    });
    assert_eq!((first_changed, second_changed), ((0, 1), (0, 1)));
}

/// What `act` changed of each of `nodes`' measurements and placements.
fn deltas(nodes: &[&Node], act: impl FnOnce()) -> Vec<(u32, u32)> {
    let before: Vec<(u32, u32)> = nodes.iter().map(|node| node.counts()).collect();
    act();
    (nodes.iter().zip(before))
        .map(|(node, (measures, layouts))| {
            (node.measures.get() - measures, node.layouts.get() - layouts)
        })
        .collect()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, oneChildRelayoutRequestAnotherRemeasure
#[test]
fn one_child_measured_again_and_another_placed_again() {
    let (root, mut host) = two_children();
    let (first, second) = (root.first(), root.second());
    let changed = deltas(&[&first, &second], || {
        first.request_remeasure();
        second.request_relayout();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(1, 1), (0, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, remeasureTwoNodesOnDifferentLayers_othersAreNotAffected
#[test]
fn two_nodes_of_different_depths_measured_again_leave_the_others() {
    let root = node(|it| {
        it.add(leaf());
        it.add(node(|it| it.add(leaf())));
    });
    let mut host = delegate(&root);
    let (first, second, second_first) = (root.first(), root.second(), root.second().first());
    let changed = deltas(&[&root, &first, &second, &second_first], || {
        first.request_remeasure();
        second_first.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (1, 1), (0, 0), (1, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, changeSizeOfTheLeaf_remeasuresUpToTheFixedSizeParent
#[test]
fn a_leaf_s_new_size_measures_up_to_the_parent_of_a_fixed_size() {
    let root = node(|it| {
        it.wrap_children.set(true);
        it.add(node(|it| {
            it.size.set(Some(DIFFERENT_SIZE));
            it.add(node(|it| {
                it.wrap_children.set(true);
                it.add(node(|it| it.size.set(Some(DIFFERENT_SIZE))));
            }));
        }));
    });
    let mut host = delegate(&root);
    let (fixed, wrapping) = (root.first(), root.first().first());
    let leaf = wrapping.first();
    let changed = deltas(&[&root, &fixed, &wrapping, &leaf], || {
        leaf.size.set(Some(DIFFERENT_SIZE_2));
        leaf.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (1, 1), (1, 1), (1, 1)]);
}

/// Two parents of a child each.
fn two_parents() -> (Node, UiHost) {
    let root = node(|it| {
        it.add(node(|it| it.add(leaf())));
        it.add(node(|it| it.add(leaf())));
    });
    let host = delegate(&root);
    (root, host)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, remeasureRequestForItemsOnTheSameLevelButDifferentParents
#[test]
fn cousins_measured_again_leave_their_parents() {
    let (root, mut host) = two_parents();
    let (first, second) = (root.first(), root.second());
    let (first_child, second_child) = (first.first(), second.first());
    let changed = deltas(&[&root, &first, &first_child, &second, &second_child], || {
        first_child.request_remeasure();
        second_child.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (0, 0), (1, 1), (0, 0), (1, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, relayoutRequestForItemsOnTheSameLevelButDifferentParents
#[test]
fn cousins_placed_again_leave_their_parents() {
    let (root, mut host) = two_parents();
    let (first, second) = (root.first(), root.second());
    let (first_child, second_child) = (first.first(), second.first());
    let changed = deltas(&[&root, &first, &first_child, &second, &second_child], || {
        first_child.request_relayout();
        second_child.request_relayout();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (0, 0), (0, 1), (0, 0), (0, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, relayoutAndRemeasureRequestForItemsOnTheSameLevelButDifferentParents
#[test]
fn a_cousin_measured_again_and_the_other_placed_again_leave_their_parents() {
    let (root, mut host) = two_parents();
    let (first, second) = (root.first(), root.second());
    let (first_child, second_child) = (first.first(), second.first());
    let changed = deltas(&[&root, &first, &first_child, &second, &second_child], || {
        first_child.request_remeasure();
        second_child.request_relayout();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (0, 0), (1, 1), (0, 0), (0, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, requestChildRemeasureDuringMeasure
#[test]
fn a_child_asked_to_measure_again_while_its_parent_measures_is_measured() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let c = child.clone();
    let changed = deltas(&[&root, &child], || {
        root.run_during_measure(true, move || c.request_remeasure());
        root.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(1, 1), (1, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, requestGrandchildRemeasureDuringMeasure
#[test]
fn a_grandchild_asked_to_measure_again_while_the_root_measures_is_measured() {
    let root = node(|it| it.add(node(|it| it.add(leaf()))));
    let mut host = delegate(&root);
    let (child, grandchild) = (root.first(), root.first().first());
    let g = grandchild.clone();
    let changed = deltas(&[&root, &child, &grandchild], || {
        root.run_during_measure(true, move || g.request_remeasure());
        root.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(1, 1), (0, 0), (1, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, requestChildRelayoutDuringMeasure
#[test]
fn a_child_asked_to_place_again_while_its_parent_measures_is_placed() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let c = child.clone();
    let changed = deltas(&[&root, &child], || {
        root.run_during_measure(true, move || c.request_relayout());
        root.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(1, 1), (0, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, requestGrandchildRelayoutDuringMeasure
#[test]
fn a_grandchild_asked_to_place_again_while_the_root_measures_is_placed() {
    let root = node(|it| it.add(node(|it| it.add(leaf()))));
    let mut host = delegate(&root);
    let (child, grandchild) = (root.first(), root.first().first());
    let g = grandchild.clone();
    let changed = deltas(&[&root, &child, &grandchild], || {
        root.run_during_measure(true, move || g.request_relayout());
        root.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(1, 1), (0, 0), (0, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, requestChildRemeasureDuringParentLayout
#[test]
fn a_child_asked_to_measure_again_while_its_parent_places_is_measured() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let c = child.clone();
    let changed = deltas(&[&root, &child], || {
        root.run_during_layout(true, move || c.request_remeasure());
        root.request_relayout();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 1), (1, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, requestGrandchildRemeasureDuringParentLayout
#[test]
fn a_grandchild_asked_to_measure_again_while_the_root_places_is_measured() {
    let root = node(|it| it.add(node(|it| it.add(leaf()))));
    let mut host = delegate(&root);
    let (child, grandchild) = (root.first(), root.first().first());
    let g = grandchild.clone();
    let changed = deltas(&[&root, &child, &grandchild], || {
        root.run_during_layout(true, move || g.request_remeasure());
        root.request_relayout();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 1), (0, 0), (1, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, requestRemeasureForCurrentlyBeingRemeasuredNode
/// As in Compose's test, the root's callback doesn't run (the root isn't measured): the child's
/// own measurement asking is
/// [`a_node_asking_for_itself_while_it_s_measured_or_placed_is_measured_or_placed_once`].
#[test]
fn a_node_asked_to_measure_again_while_it_s_being_measured_is_measured_once() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let c = child.clone();
    let changed = deltas(&[&root, &child], || {
        root.run_during_measure(true, move || c.request_remeasure());
        child.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (1, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, requestRelayoutForCurrentlyBeingRemeasuredNode
/// As in Compose's test, the root's callback doesn't run (the root isn't measured).
#[test]
fn a_node_asked_to_place_again_while_it_s_being_measured_is_measured_and_placed_once() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let c = child.clone();
    let changed = deltas(&[&root, &child], || {
        root.run_during_measure(true, move || c.request_relayout());
        child.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (1, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, requestRemeasureForCurrentlyBeingRelayoutNode
#[test]
fn a_node_asked_to_measure_again_while_it_s_being_placed_is_measured_and_placed_again() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let c = child.clone();
    let changed = deltas(&[&root, &child], || {
        child.run_during_layout(true, move || c.request_remeasure());
        child.request_relayout();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (1, 2)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, requestRelayoutForCurrentlyBeingRelayoutNode
/// As in Compose's test, the root's callback doesn't run (the root isn't placed).
#[test]
fn a_node_asked_to_place_again_while_its_parent_places_is_placed_once() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let c = child.clone();
    let changed = deltas(&[&root, &child], || {
        root.run_during_layout(true, move || c.request_relayout());
        child.request_relayout();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (0, 1)]);
}

/// A root, a parent and a child, under root constraints from `max`.
fn three_levels(
    build_parent: impl FnOnce(&Node),
) -> (Node, UiHost, compose_runtime::OwnedState<i32>) {
    let root = node(|it| {
        it.add(node(|it| {
            build_parent(it);
            it.add(leaf());
        }))
    });
    let max = mutable_state_of(100);
    let host = delegate_in(&root, Some(max.handle()));
    (root, host, max)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, changingParentParamsToTheSameValue_noRemeasures
#[test]
fn the_same_root_constraints_again_measure_nothing() {
    let (root, mut host, max) = three_levels(|_| {});
    let (parent, child) = (root.first(), root.first().first());
    let changed = deltas(&[&root, &parent, &child], || {
        max.set(100);
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (0, 0), (0, 0)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, changingParentConstraints_remeasureSubTree
#[test]
fn new_root_constraints_measure_the_tree_again() {
    let (root, mut host, max) = three_levels(|_| {});
    let (parent, child) = (root.first(), root.first().first());
    let changed = deltas(&[&root, &parent, &child], || {
        max.set(DIFFERENT_SIZE);
        assert!(measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(1, 1), (1, 1), (1, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, changingParentConstraints_remeasureOnlyAffectedNodes
#[test]
fn new_root_constraints_measure_again_only_what_they_reach() {
    let (root, mut host, max) = three_levels(|it| it.size.set(Some(DIFFERENT_SIZE_2)));
    let (parent, child) = (root.first(), root.first().first());
    let changed = deltas(&[&parent, &child], || {
        max.set(DIFFERENT_SIZE);
        assert!(measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(1, 1), (0, 0)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, requestRemeasureTriggersModifierRemeasure
#[test]
fn a_node_asked_to_measure_again_measures_its_layout_modifier() {
    // The test's `SpyLayoutModifier`: a layout modifier counting its measurements.
    let spy_measures = Rc::new(Cell::new(0u32));
    let s = spy_measures.clone();
    let spy = Modifier.layout(move |scope, measurable, constraints| {
        bump(&s);
        let placeable = scope.measure(measurable, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    });
    let root = node(|it| it.add(node(|it| *it.modifier.borrow_mut() = Some(spy))));
    let mut host = delegate(&root);
    let before = spy_measures.get();
    root.first().request_remeasure();
    assert!(!measure_and_layout(&mut host, &root));
    assert_eq!(spy_measures.get(), before + 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, relayoutDependingOnRemeasuredChild
/// Compose's test also checks the child is measured before its parent's block runs (its
/// `layoutPending` there): the block placing it reads its new placeable.
#[test]
fn a_parent_placed_again_with_its_child_measured_again_is_placed_once() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let seen = Rc::new(Cell::new(None));
    let (c, m) = (child.clone(), seen.clone());
    let before = child.counts();
    // Measured, not yet placed (`layoutPending`): what the child's counts say there.
    root.run_during_layout(true, move || {
        let (measures, layouts) = c.counts();
        m.set(Some((measures - before.0, layouts - before.1)));
    });
    let changed = deltas(&[&root, &child], || {
        child.request_remeasure();
        root.request_relayout();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 1), (1, 1)]);
    assert_eq!(seen.get(), Some((1, 0)));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, relayoutDependingOnRemeasuredChild_parentRemeasuredBecauseOfChangedSize
#[test]
fn a_parent_placed_again_with_its_child_s_size_changed_is_measured_again() {
    let root = node(|it| {
        it.wrap_children.set(true);
        it.add(leaf());
    });
    let mut host = delegate(&root);
    let child = root.first();
    let changed = deltas(&[&root, &child], || {
        child.size.set(Some(DIFFERENT_SIZE));
        child.request_remeasure();
        root.request_relayout();
        assert!(measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(1, 1), (1, 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, hasRelayoutNodes
#[test]
fn a_request_is_work_for_the_next_frame_till_it_s_done() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    assert!(!host.has_pending_work());
    root.first().request_remeasure();
    assert!(host.has_pending_work());
    host.frame();
    assert!(!host.has_pending_work());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, remeasuringNodeSecondTimeWithinTheSameIteration
#[test]
fn a_node_asked_to_measure_again_after_it_was_measured_in_the_frame_is_measured_again() {
    let (node1, node2, node3, node4, node5) =
        (node(|it| it.size.set(Some(50))), Rc::new(N::default()), leaf(), leaf(), leaf());
    node2.add(node3.clone());
    node1.add(node2.clone());
    node1.add(node4.clone());
    let root = node(|it| {
        it.size.set(Some(100));
        it.add(node1.clone());
        it.add(node5.clone());
    });
    let mut host = delegate(&root);
    let changed = deltas(&[&root, &node1, &node2, &node3, &node4, &node5], || {
        root.request_remeasure();
        // The root's new size measures node1 and node5 again.
        root.size.set(Some(50));
        // node2 isn't asked: node3 is measured by itself.
        node3.request_remeasure();
        // node5 asks again for node1, measured already.
        let n1 = node1.clone();
        node5.run_during_measure(true, move || n1.request_remeasure());
        host.frame();
    });
    let measured: Vec<u32> = changed.iter().map(|it| it.0).collect();
    assert_eq!(measured, [1, 2, 0, 1, 0, 1]);
}

/// Review: what Compose's `requestRemeasureForCurrentlyBeingRemeasuredNode`,
/// `requestRelayoutForCurrentlyBeingRemeasuredNode` and `requestRelayoutForCurrentlyBeingRelayoutNode`
/// mean to ask, their callbacks on the node itself (Compose's are on the root, which isn't
/// measured or placed): Compose's `requestRemeasure` while `Measuring` and `requestRelayout` while
/// `Measuring` or `LayingOut` are swallowed, what runs seeing what asked.
#[test]
fn a_node_asking_for_itself_while_it_s_measured_or_placed_is_measured_or_placed_once() {
    let root = node(|it| it.add(leaf()));
    let mut host = delegate(&root);
    let child = root.first();
    let mut changed = Vec::new();
    let c = child.clone();
    changed.extend(deltas(&[&root, &child], || {
        child.run_during_measure(true, move || c.request_remeasure());
        child.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    }));
    let c = child.clone();
    changed.extend(deltas(&[&root, &child], || {
        child.run_during_measure(true, move || c.request_relayout());
        child.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    }));
    let c = child.clone();
    changed.extend(deltas(&[&root, &child], || {
        child.run_during_layout(true, move || c.request_relayout());
        child.request_relayout();
        assert!(!measure_and_layout(&mut host, &root));
    }));
    assert_eq!(changed, [(0, 0), (1, 1), (0, 0), (1, 1), (0, 0), (0, 1)]);
}

/// Review: a request made while the frame measures or places, done in that frame, leaves no
/// work (Compose's `hasPendingMeasureOrLayout` false, and its request returns
/// `!duringFullMeasureLayoutPass`, so it asks the owner for no other layout).
#[test]
fn a_request_made_and_done_in_a_frame_leaves_no_work() {
    let root = node(|it| it.add(node(|it| it.add(leaf()))));
    let mut host = delegate(&root);
    let (child, grandchild) = (root.first(), root.first().first());
    let g = grandchild.clone();
    let changed = deltas(&[&root, &child, &grandchild], || {
        root.run_during_measure(true, move || g.request_remeasure());
        root.request_remeasure();
        host.frame();
    });
    assert_eq!(changed, [(1, 1), (0, 0), (1, 1)]);
    let c = child.clone();
    let changed = deltas(&[&root, &child, &grandchild], || {
        child.run_during_layout(true, move || c.request_remeasure());
        child.request_relayout();
        host.frame();
    });
    assert_eq!(changed, [(0, 0), (1, 2), (0, 0)]);
    let next = deltas(&[&root, &child, &grandchild], || {
        host.frame();
    });
    assert_eq!(next, [(0, 0); 3], "the frame did it all");
    let g = grandchild.clone();
    root.run_during_measure(true, move || g.request_remeasure());
    root.request_remeasure();
    host.frame();
    assert!(!host.has_pending_work());
}

/// Review: a child its parent never measured, asked to measure again, measures nothing: Compose's
/// child is still `measurePending` from its creation, so `requestRemeasure` returns at once and
/// its parent isn't asked either.
#[test]
fn a_child_its_parent_never_measured_asked_to_measure_again_measures_nothing() {
    let child = node(|it| it.size.set(Some(40)));
    let root = node(|it| {
        it.no_measure.set(true);
        it.size.set(Some(100));
        it.add(child.clone());
    });
    let mut host = delegate(&root);
    assert_eq!((root.counts(), child.counts()), ((1, 1), (0, 0)));
    let changed = deltas(&[&root, &child], || {
        child.size.set(Some(60));
        child.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (0, 0)]);
}

/// Review: a child its parent measured, then stopped measuring, asked to measure again with a
/// new size measures nothing until its parent measures it again: Compose's isn't placed and its
/// `measuredByParent` is `NotUsed`, so `requestRemeasure` only marks it `measurePending`
/// (`canAffectPlacedParent` false), and its parent's next measurement measures it.
#[test]
fn a_child_its_parent_stopped_measuring_asked_to_measure_again_waits_for_its_parent() {
    let child = node(|it| it.size.set(Some(40)));
    let root = node(|it| {
        it.size.set(Some(100));
        it.add(child.clone());
    });
    let mut host = delegate(&root);
    assert_eq!(child.counts(), (1, 1));
    root.no_measure.set(true);
    root.request_remeasure();
    host.frame();
    assert_eq!(child.counts(), (1, 1));

    let changed = deltas(&[&root, &child], || {
        child.size.set(Some(30));
        child.request_remeasure();
        assert!(!measure_and_layout(&mut host, &root));
    });
    assert_eq!(changed, [(0, 0), (0, 0)]);

    root.no_measure.set(false);
    root.request_remeasure();
    let changed = deltas(&[&root, &child], || {
        host.frame();
    });
    assert_eq!(changed, [(1, 1), (1, 1)]);
}
