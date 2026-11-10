//! Review of D1931 (a cancel tells a hit path innermost first) against Compose's `HitPathTracker`:
//! paths of unequal depth, several pointer input nodes on one layout node, a delegating node's
//! delegates (D2120), nodes removed around a cancel, and what a pointer's next event does (D2121).

use crate::support;

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use support::*;

type Log = Rc<RefCell<Vec<String>>>;

fn finger(id: u64, x: f32, y: f32) -> PointerEvent {
    PointerEvent::moved(Offset::new(x, y), true).with_pointer(PointerId(id), PointerType::Touch)
}

fn finger_down(id: u64, x: f32, y: f32) -> PointerEvent {
    PointerEvent::press(Offset::new(x, y)).with_pointer(PointerId(id), PointerType::Touch)
}

fn finger_up(id: u64, x: f32, y: f32) -> PointerEvent {
    PointerEvent::release(Offset::new(x, y)).with_pointer(PointerId(id), PointerType::Touch)
}

/// Writes down "<name> <kind>" for each event in the main pass (" down" when a pointer of it comes
/// down), and "<name> cancel" when cancelled.
struct ProbeNode(&'static str, Log);

impl ModifierNode for ProbeNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for ProbeNode {
    fn takes_whole_events(&self) -> bool {
        true
    }
    fn on_pointer_events(
        &mut self,
        event: &PointerEvents,
        pass: PointerEventPass,
        _: IntSize,
        _: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Main {
            let down = event.changes.iter().any(|it| it.changed_to_down_ignore_consumed());
            let down = if down { " down" } else { "" };
            self.1.borrow_mut().push(format!("{} {:?}{down}", self.0, event.kind));
        }
    }
    fn on_cancel_pointer_input(&mut self) {
        self.1.borrow_mut().push(format!("{} cancel", self.0));
    }
}

struct Probe(&'static str, Log);

impl ModifierNodeElement for Probe {
    type Node = ProbeNode;
    fn create(&self) -> ProbeNode {
        ProbeNode(self.0, self.1.clone())
    }
    fn update(&self, _: &mut ProbeNode) {}
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

fn probe(name: &'static str, log: &Log) -> Modifier {
    Modifier.then_custom(Probe(name, log.clone()))
}

/// `root` around `parent`, a row of `mid` (around `leaf_a`, 50 wide) and `child_b` (50 wide).
fn unequal_depths(log: &Log) -> compose_ui::platform::UiHost {
    let log = log.clone();
    host(move || {
        let log = log.clone();
        Stack!(modifier = probe("root", &log), {
            let log = log.clone();
            Row!(modifier = probe("parent", &log), {
                let inner = log.clone();
                Stack!(modifier = probe("mid", &log), {
                    Leaf(Modifier.size(50.dp()).then(probe("leaf_a", &inner)));
                });
                Leaf(Modifier.size(50.dp()).then(probe("child_b", &log)));
            });
        });
    })
}

/// Two pointers on branches of unequal depth under one parent: each branch innermost first, in the
/// order the pointers came down (Compose's tree keeps its children in the order paths were added),
/// then the shared parent and root once.
#[test]
fn branches_of_unequal_depth_cancel_each_before_their_shared_parent() {
    for deep_first in [true, false] {
        let log = Log::default();
        let mut host = unequal_depths(&log);
        let (deep, shallow) = (finger_down(1, 10.0, 10.0), finger_down(2, 60.0, 10.0));
        // One at a time: an event's changes go in the order of their ids.
        if deep_first {
            host.send_pointer(deep);
            host.send_pointer(shallow);
        } else {
            host.send_pointer(shallow);
            host.send_pointer(deep);
        }
        log.borrow_mut().clear();
        host.with_tree(|tree| tree.cancel_pointers());
        let expected = if deep_first {
            ["leaf_a cancel", "mid cancel", "child_b cancel", "parent cancel", "root cancel"]
        } else {
            ["child_b cancel", "leaf_a cancel", "mid cancel", "parent cancel", "root cancel"]
        };
        assert_eq!(*log.borrow(), expected, "deep_first={deep_first}");
    }
}

/// One pointer's path a prefix of another's: the longer one's end first, the shared nodes once,
/// whichever came down first.
#[test]
fn a_path_inside_another_cancels_the_longer_ones_end_first() {
    for shallow_first in [true, false] {
        let log = Log::default();
        let seen = log.clone();
        let mut host = host(move || {
            let log = seen.clone();
            Stack!(modifier = Modifier.size(100.dp()).then(probe("root", &log)), {
                let log = log.clone();
                Stack!(modifier = probe("mid", &log), {
                    Leaf(Modifier.size(40.dp()).then(probe("leaf", &log)));
                });
            });
        });
        let (shallow, deep) = (finger_down(1, 70.0, 10.0), finger_down(2, 10.0, 10.0));
        if shallow_first {
            host.send_pointer(shallow);
            host.send_pointer(deep);
        } else {
            host.send_pointer(deep);
            host.send_pointer(shallow);
        }
        log.borrow_mut().clear();
        host.with_tree(|tree| tree.cancel_pointers());
        assert_eq!(
            *log.borrow(),
            ["leaf cancel", "mid cancel", "root cancel"],
            "shallow_first={shallow_first}"
        );
    }
}

/// Three pointers over two levels of branches: every subtree before what's around it.
#[test]
fn branches_at_two_levels_cancel_every_subtree_before_its_parent() {
    let log = Log::default();
    let seen = log.clone();
    let mut host = host(move || {
        let log = seen.clone();
        Row!(modifier = probe("parent", &log), {
            let log = log.clone();
            Leaf(Modifier.size(40.dp()).then(probe("shallow", &log)));
            Row!(modifier = probe("mid", &log), {
                Leaf(Modifier.size(40.dp()).then(probe("deep1", &log)));
                Leaf(Modifier.size(40.dp()).then(probe("deep2", &log)));
            });
        });
    });
    host.send_pointer(finger_down(1, 90.0, 10.0));
    host.send_pointer(finger_down(2, 10.0, 10.0));
    host.send_pointer(finger_down(3, 50.0, 10.0));
    log.borrow_mut().clear();
    host.with_tree(|tree| tree.cancel_pointers());
    assert_eq!(
        *log.borrow(),
        ["deep2 cancel", "deep1 cancel", "mid cancel", "shallow cancel", "parent cancel"]
    );
}

/// Several pointer input nodes on each of two layout nodes: the inner layout node's, innermost
/// first, then the outer's, by both cancels.
#[test]
fn several_nodes_on_each_layout_node_cancel_innermost_first() {
    for one in [false, true] {
        let log = Log::default();
        let seen = log.clone();
        let mut host = host(move || {
            let log = seen.clone();
            let parent = probe("parent1", &log).then(probe("parent2", &log));
            Stack!(modifier = parent, {
                Leaf(
                    Modifier.size(50.dp()).then(probe("child1", &log)).then(probe("child2", &log)),
                );
            });
        });
        host.send_pointer(finger_down(1, 10.0, 10.0));
        log.borrow_mut().clear();
        if one {
            host.with_tree(|tree| tree.cancel_pointer(PointerId(1)));
        } else {
            host.with_tree(|tree| tree.cancel_pointers());
        }
        assert_eq!(
            *log.borrow(),
            ["child2 cancel", "child1 cancel", "parent2 cancel", "parent1 cancel"],
            "one={one}"
        );
    }
}

/// A node shared by two paths that don't start alike (the outer node's bounds don't hold the
/// first pointer; an unclipped `offset` puts the inner node and its children past them): told once,
/// where the first path has it. Compose's tree merges paths only where they start alike, so it
/// tells `inner` there and again after `child_a` (D1931: a node told once).
#[test]
fn a_node_on_paths_that_start_apart_is_told_once_where_the_first_has_it() {
    let log = Log::default();
    let seen = log.clone();
    let mut host = host(move || {
        let log = seen.clone();
        let chain = Modifier
            .size(50.dp())
            .then(probe("outer", &log))
            .offset(40.dp(), 0.dp())
            .size(50.dp())
            .then(probe("inner", &log));
        Row!(modifier = chain, {
            Leaf(Modifier.size(20.dp()).then(probe("child_a", &log)));
            Leaf(Modifier.size(30.dp()).then(probe("child_b", &log)));
        });
    });
    // `outer` holds x in [0, 50), `inner` [40, 90): `child_a` [40, 60), `child_b` [60, 90).
    host.send_pointer(finger_down(1, 75.0, 10.0));
    host.send_pointer(finger_down(2, 45.0, 10.0));
    assert_eq!(
        *log.borrow(),
        [
            "child_b Press down",
            "inner Press down",
            // The second press: `inner` hears it on each path, as on each of Compose's two.
            "child_b Press",
            "inner Press",
            "child_a Press down",
            "inner Press down",
            "outer Press down",
        ]
    );
    log.borrow_mut().clear();
    host.with_tree(|tree| tree.cancel_pointers());
    assert_eq!(*log.borrow(), ["child_b cancel", "inner cancel", "child_a cancel", "outer cancel"]);
}

/// A delegating node with three pointer input delegates, between two nodes of its own.
struct ThreeDelegates(Log);

struct ThreeDelegatesNode {
    delegates: Delegates,
}

impl ModifierNodeElement for ThreeDelegates {
    type Node = ThreeDelegatesNode;
    fn create(&self) -> ThreeDelegatesNode {
        let mut delegates = Delegates::new();
        delegates.delegate(ProbeNode("del1", self.0.clone()));
        delegates.delegate(ProbeNode("del2", self.0.clone()));
        delegates.delegate(ProbeNode("del3", self.0.clone()));
        ThreeDelegatesNode { delegates }
    }
    fn update(&self, _: &mut ThreeDelegatesNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for ThreeDelegatesNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
}

/// A delegating node's delegates are one node on Compose's hit path: its `dispatchCancel` tells
/// them in delegation order (`dispatchForKind`), as the main pass does, after what's inside it
/// and before what's around it; by both cancels (D2120).
#[test]
fn a_delegating_nodes_delegates_cancel_in_delegation_order() {
    for one in [false, true] {
        let log = Log::default();
        let seen = log.clone();
        let mut host = host(move || {
            let log = seen.clone();
            Leaf(
                Modifier
                    .size(50.dp())
                    .then(probe("outer", &log))
                    .then_custom(ThreeDelegates(log.clone()))
                    .then(probe("inner", &log)),
            );
        });
        host.send_pointer(finger_down(4, 10.0, 10.0));
        // The main pass, inside out: the delegates in delegation order.
        assert_eq!(
            *log.borrow(),
            [
                "inner Press down",
                "del1 Press down",
                "del2 Press down",
                "del3 Press down",
                "outer Press down"
            ]
        );
        log.borrow_mut().clear();
        if one {
            host.with_tree(|tree| tree.cancel_pointer(PointerId(4)));
        } else {
            host.with_tree(|tree| tree.cancel_pointers());
        }
        assert_eq!(
            *log.borrow(),
            ["inner cancel", "del1 cancel", "del2 cancel", "del3 cancel", "outer cancel"],
            "one={one}"
        );
    }
}

/// Clears `remove` when cancelled: what it's in goes at the next frame.
struct RemovesOnCancel(MutableState<bool>, Log);

struct RemovesOnCancelNode(MutableState<bool>, Log);

impl ModifierNodeElement for RemovesOnCancel {
    type Node = RemovesOnCancelNode;
    fn create(&self) -> RemovesOnCancelNode {
        RemovesOnCancelNode(self.0, self.1.clone())
    }
    fn update(&self, node: &mut RemovesOnCancelNode) {
        node.0 = self.0;
    }
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for RemovesOnCancelNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for RemovesOnCancelNode {
    fn on_pointer_event(
        &mut self,
        _: &mut PointerEvent,
        _: PointerEventPass,
        _: IntSize,
        _: &mut EventContext<'_>,
    ) {
    }
    fn on_cancel_pointer_input(&mut self) {
        self.1.borrow_mut().push("child cancel".into());
        self.0.set(false);
    }
}

/// A child whose cancel removes it and its parent: the parent is still told, after it; the next
/// frame removes both, and a press after hits what's there then.
#[test]
fn a_cancel_that_removes_its_nodes_tells_the_rest_of_the_path() {
    let log = Log::default();
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let seen = log.clone();
    let mut host = host(move || {
        let log = seen.clone();
        Stack!(modifier = Modifier.size(100.dp()).then(probe("root", &log)), {
            if handle.get() {
                let log = log.clone();
                Stack!(modifier = probe("parent", &log), {
                    Leaf(Modifier.size(50.dp()).then_custom(RemovesOnCancel(handle, log.clone())));
                });
            }
        });
    });
    host.send_pointer(finger_down(1, 10.0, 10.0));
    log.borrow_mut().clear();
    host.with_tree(|tree| tree.cancel_pointers());
    assert_eq!(*log.borrow(), ["child cancel", "parent cancel", "root cancel"]);
    host.frame();
    assert!(!shown.get());
    log.borrow_mut().clear();
    host.send_pointer(finger_down(2, 10.0, 10.0));
    host.with_tree(|tree| tree.cancel_pointers());
    assert_eq!(*log.borrow(), ["root Press down", "root cancel"]);
}

/// A captured leaf removed before the cancel isn't told (it left with its node); the parent is,
/// once.
#[test]
fn a_cancel_after_a_captured_leaf_left_tells_what_stayed() {
    let log = Log::default();
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let seen = log.clone();
    let mut host = host(move || {
        let log = seen.clone();
        Stack!(modifier = Modifier.size(50.dp()).then(probe("parent", &log)), {
            if handle.get() {
                Leaf(Modifier.size(50.dp()).then(probe("leaf", &log)));
            }
        });
    });
    host.send_pointer(finger_down(1, 10.0, 10.0));
    shown.set(false);
    host.frame();
    log.borrow_mut().clear();
    host.with_tree(|tree| tree.cancel_pointers());
    assert_eq!(*log.borrow(), ["parent cancel"]);
}

/// Compose's `HitPathTracker.removePointerInputModifierNode`: a node leaving mid-gesture is taken
/// off the paths, and it and what's below it on them are told the cancel there (`dispatchCancel`),
/// hearing that gesture no more. compose-rs lets the node go untold, and what was below it keeps
/// hearing the gesture.
#[test]
fn a_node_leaving_mid_gesture_cancels_what_was_below_it() {
    let log = Log::default();
    let with_outer = mutable_state_of(true);
    let handle = with_outer.handle();
    let seen = log.clone();
    let mut host = host(move || {
        let log = seen.clone();
        let parent = if handle.get() { probe("parent", &log) } else { Modifier };
        Stack!(modifier = parent, {
            Leaf(Modifier.size(50.dp()).then(probe("child", &log)));
        });
    });
    host.send_pointer(finger_down(1, 10.0, 10.0));
    log.borrow_mut().clear();
    with_outer.set(false);
    host.frame();
    assert_eq!(*log.borrow(), ["child cancel", "parent cancel"]);
    log.borrow_mut().clear();
    host.send_pointer(finger(1, 12.0, 10.0));
    host.send_pointer(finger_up(1, 12.0, 10.0));
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
}

/// After a cancel, a pointer still down comes down again at its next move (Compose's processor
/// forgot it), and its path is kept as a press's: its drag past the node and the next cancel still
/// reach the node, and its release ends it (D2121).
#[test]
fn a_pointer_down_through_a_cancel_keeps_the_path_it_comes_down_on_again() {
    for one in [false, true] {
        let log = Log::default();
        let seen = log.clone();
        let mut host = host(move || {
            let log = seen.clone();
            Leaf(Modifier.size(50.dp()).then(probe("box", &log)));
        });
        host.send_pointer(finger_down(3, 10.0, 10.0));
        if one {
            host.with_tree(|tree| tree.cancel_pointer(PointerId(3)));
        } else {
            host.with_tree(|tree| tree.cancel_pointers());
        }
        log.borrow_mut().clear();
        host.send_pointer(finger(3, 12.0, 10.0));
        host.send_pointer(finger(3, 80.0, 10.0));
        host.with_tree(|tree| tree.cancel_pointers());
        assert_eq!(*log.borrow(), ["box Move down", "box Move", "box cancel"], "one={one}");

        // Down again, past the node to its release: the release reaches it, and ends the capture.
        log.borrow_mut().clear();
        host.send_pointer(finger(3, 12.0, 10.0));
        host.send_pointer(finger_up(3, 80.0, 10.0));
        host.with_tree(|tree| tree.cancel_pointers());
        assert_eq!(*log.borrow(), ["box Move down", "box Release"], "one={one}");
    }
}

/// Compose's `processCancel` cancels every node of its tree, a resting mouse's hover path too, and
/// clears it: the mouse's next move enters them again. compose-rs's cancels only what pointers
/// pressed, and keeps the hover: the next move is a move.
#[test]
fn a_cancel_cancels_the_hover_and_the_next_move_enters_again() {
    let log = Log::default();
    let seen = log.clone();
    let mut host = host(move || {
        let log = seen.clone();
        Leaf(Modifier.size(50.dp()).then(probe("box", &log)));
    });
    host.send_pointer(PointerEvent::moved(Offset::new(10.0, 10.0), false));
    assert_eq!(*log.borrow(), ["box Enter"]);
    log.borrow_mut().clear();
    host.with_tree(|tree| tree.cancel_pointers());
    host.send_pointer(PointerEvent::moved(Offset::new(12.0, 10.0), false));
    assert_eq!(*log.borrow(), ["box cancel", "box Enter"]);
}

/// A touch capture and a mouse hover sharing a parent: `cancel_pointers` cancels both children
/// before the shared parent (once), and clears the resting cursor so a layout frame before the
/// next move does not re-enter the hover.
#[test]
fn a_cancel_with_a_touch_and_a_hover_cancels_children_before_their_shared_parent_and_forgets_the_hover()
 {
    let log = Log::default();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    let seen = log.clone();
    let mut host = host(move || {
        let _ = handle.get();
        let log = seen.clone();
        Row!(modifier = probe("parent", &log), {
            Leaf(Modifier.size(50.dp()).then(probe("child_a", &log)));
            Leaf(Modifier.size(50.dp()).then(probe("child_b", &log)));
        });
    });
    host.send_pointer(finger_down(1, 10.0, 10.0));
    host.send_pointer(PointerEvent::moved(Offset::new(60.0, 10.0), false));
    log.borrow_mut().clear();
    host.with_tree(|tree| tree.cancel_pointers());
    assert_eq!(*log.borrow(), ["child_a cancel", "child_b cancel", "parent cancel"]);
    log.borrow_mut().clear();
    tick.set(1);
    host.frame();
    assert!(log.borrow().is_empty(), "frame after cancel must not re-enter: {:?}", log.borrow());
    host.send_pointer(PointerEvent::moved(Offset::new(62.0, 10.0), false));
    assert_eq!(*log.borrow(), ["child_b Enter", "parent Enter"]);
}

/// A hovered node leaving (either its layout node or a modifier entry on it) cancels it and what
/// was below it on the hover path, while what's above it stays hovered (the next move moves there,
/// not entering again).
#[test]
fn a_hovered_node_leaving_cancels_it_and_what_was_below_it_keeping_its_parent_hovered() {
    struct KeyedProbe<const SLOT: usize>(&'static str, Log);
    impl<const SLOT: usize> ModifierNodeElement for KeyedProbe<SLOT> {
        type Node = ProbeNode;
        fn create(&self) -> ProbeNode {
            ProbeNode(self.0, self.1.clone())
        }
        fn update(&self, node: &mut ProbeNode) {
            node.0 = self.0;
        }
        fn same(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    fn slot_probe<const SLOT: usize>(name: &'static str, log: &Log) -> Modifier {
        Modifier.then_custom(KeyedProbe::<SLOT>(name, log.clone()))
    }

    for by_chain in [false, true] {
        let log = Log::default();
        let keep = mutable_state_of(true);
        let handle = keep.handle();
        let seen = log.clone();
        let mut host = host(move || {
            let log = seen.clone();
            if by_chain {
                let mid = if handle.get() { slot_probe::<1>("mid", &log) } else { Modifier };
                Leaf(
                    Modifier
                        .size(50.dp())
                        .then(slot_probe::<0>("parent", &log))
                        .then(mid)
                        .then(slot_probe::<2>("leaf", &log)),
                );
            } else {
                let inner = log.clone();
                Stack!(modifier = Modifier.size(50.dp()).then(probe("parent", &log)), {
                    if handle.get() {
                        let leaf_log = inner.clone();
                        Stack!(modifier = probe("mid", &inner), {
                            Leaf(Modifier.size(50.dp()).then(probe("leaf", &leaf_log)));
                        });
                    }
                });
            }
        });
        host.send_pointer(PointerEvent::moved(Offset::new(10.0, 10.0), false));
        log.borrow_mut().clear();
        keep.set(false);
        host.frame();
        if by_chain {
            // `mid` and `leaf` cancel as `mid` leaves the chain; `refresh_hover` then enters `leaf`
            // under the resting cursor while `parent` stayed hovered throughout.
            assert_eq!(*log.borrow(), ["leaf cancel", "mid cancel", "leaf Enter"]);
            log.borrow_mut().clear();
            host.send_pointer(PointerEvent::moved(Offset::new(12.0, 10.0), false));
            assert_eq!(*log.borrow(), ["leaf Move", "parent Move"]);
        } else {
            assert_eq!(*log.borrow(), ["leaf cancel", "mid cancel"]);
            log.borrow_mut().clear();
            host.send_pointer(PointerEvent::moved(Offset::new(12.0, 10.0), false));
            assert_eq!(*log.borrow(), ["parent Move"]);
        }
    }
}

/// Pressing outside a non-focusable `Popup` hits its pass-through dismiss catcher and the content
/// beneath it; when the popup closes mid-gesture, its catcher leaves without cancelling the
/// underlying window's gesture.
#[test]
fn a_pass_through_popup_closing_on_an_outside_press_leaves_the_gesture_beneath_it_alive() {
    let log = Log::default();
    let open = mutable_state_of(true);
    let handle = open.handle();
    let dismiss = open.handle();
    let seen = log.clone();
    let mut host = host(move || {
        let log = seen.clone();
        Stack!(modifier = Modifier.size(100.dp()), {
            if handle.get() {
                Popup!(
                    on_dismiss_request = Some(move || dismiss.set(false)),
                    properties = PopupProperties::default(),
                    {
                        Leaf(Modifier.size(20.dp()));
                    }
                );
            }
            Leaf(Modifier.offset(40.dp(), 40.dp()).size(40.dp()).then(probe("beneath", &log)));
        });
    });
    host.send_pointer(finger_down(1, 50.0, 50.0));
    assert!(!open.get(), "outside press dismissed the popup");
    assert_eq!(*log.borrow(), ["beneath Press down"]);
    log.borrow_mut().clear();
    host.frame();
    assert!(log.borrow().is_empty(), "closing popup must not cancel beneath: {:?}", log.borrow());
    host.send_pointer(finger(1, 52.0, 50.0));
    host.send_pointer(finger_up(1, 52.0, 50.0));
    assert_eq!(*log.borrow(), ["beneath Move", "beneath Release"]);
}
