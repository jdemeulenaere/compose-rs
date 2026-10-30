//! Compose's `MovableContentTests`, on one composition (those across compositions come with step
//! C6), rewritten: content moves with its state, its observers and its nodes.

#![allow(non_snake_case)]
// A call in a closure and in each branch is a place of its own: that's what these tests move.
#![allow(clippy::redundant_closure, clippy::if_same_then_else)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::platform::{self, Applier};
use compose_runtime::*;

use crate::mock::*;

#[composable]
fn Row(content: impl Fn() + 'static) {
    ComposeNodeWithContent::<ViewApplier>(view("Row"), |_| {}, || content());
}

#[composable]
fn Column(content: impl Fn() + 'static) {
    ComposeNodeWithContent::<ViewApplier>(view("Column"), |_| {}, || content());
}

#[composable(non_restartable)]
fn Marker() {
    ComposeNode::<ViewApplier>(view("Marker"), |_| {});
}

#[composable(non_restartable)]
fn MarkerOf(value: i32) {
    ComposeNode::<ViewApplier>(view("Marker"), |updater| {
        updater.set(value.to_string(), set_attribute("value"))
    });
}

#[composable]
fn Empty() {}

fn row(v: &mut Expected, children: impl FnOnce(&mut Expected)) {
    v.view("Row", &[], children);
}

fn column(v: &mut Expected, children: impl FnOnce(&mut Expected)) {
    v.view("Column", &[], children);
}

fn marker(v: &mut Expected) {
    v.view("Marker", &[], |_| {});
}

fn marker_of(v: &mut Expected, value: i32) {
    v.view("Marker", &[("value", &value.to_string())], |_| {});
}

/// The id of the first view named `name`.
fn first_named(test: &CompositionTest, name: &str) -> u64 {
    test.with_views(|views| {
        let mut stack: Vec<_> = views.roots().into_iter().rev().collect();
        while let Some(node) = stack.pop() {
            let view = views.tree.get(node).expect("a live view");
            if view.name == name {
                return view.id;
            }
            let children: Vec<_> = views.tree.children(node).collect();
            stack.extend(children.into_iter().rev());
        }
        panic!("no view named {name}");
    })
}

/// `RememberedObject`: an observer whose counts say it stays alive.
#[derive(Clone, Default)]
struct Remembered {
    count: Rc<Cell<i32>>,
    died: Rc<Cell<bool>>,
}

impl Remembered {
    fn is_live(&self) -> bool {
        self.count.get() > 0
    }
}

struct Tracked(Remembered);

impl RememberObserver for Tracked {
    fn on_remembered(&self) {
        assert!(!self.0.died.get(), "Remember observer was resurrected");
        self.0.count.set(self.0.count.get() + 1);
    }
    fn on_forgotten(&self) {
        assert!(self.0.count.get() > 0, "forgotten more times than remembered");
        self.0.count.set(self.0.count.get() - 1);
        if self.0.count.get() == 0 {
            self.0.died.set(true);
        }
    }
    fn on_abandoned(&self) {
        panic!("abandoned, where it was remembered");
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, testMovableContentSharesState
#[test]
fn content_moved_keeps_its_state() {
    let mut test = composition_test();
    let portrait = mutable_state_of(false);
    let states: Rc<RefCell<Vec<MutableState<i32>>>> = Rc::default();
    let seen = states.clone();
    let content = movable_content_of(move || {
        let private = remember(|| mutable_state_of(0));
        seen.borrow_mut().push(private);
        Text("Some text");
        Text("Some other text");
    });
    let p = portrait.handle();
    test.compose(move || {
        let content = content.clone();
        if p.get() {
            Column!(move || content());
        } else {
            Row!(move || content());
        }
    });
    let p = portrait.handle();
    test.validate(move |v| {
        let value = |v: &mut Expected| {
            v.text("Some text");
            v.text("Some other text");
        };
        if p.get() {
            column(v, value);
        } else {
            row(v, value);
        }
    });
    let first = states.borrow()[0];
    portrait.set(true);
    test.expect_changes();
    test.revalidate();
    assert_eq!(*states.borrow().last().expect("composed"), first, "the state should be shared");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentPreservesNodes
#[test]
fn content_moved_keeps_its_nodes() {
    let mut test = composition_test();
    let portrait = mutable_state_of(false);
    let content = movable_content_of(|| {
        Text("Some text");
        Text("Some other text");
    });
    let p = portrait.handle();
    test.compose(move || {
        let content = content.clone();
        if p.get() {
            Column!(move || content());
        } else {
            Row!(move || content());
        }
    });
    let first = first_named(&test, "text");
    portrait.set(true);
    test.expect_changes();
    let p = portrait.handle();
    test.validate(move |v| {
        let value = |v: &mut Expected| {
            v.text("Some text");
            v.text("Some other text");
        };
        if p.get() {
            column(v, value);
        } else {
            row(v, value);
        }
    });
    assert_eq!(first_named(&test, "text"), first, "the text should be the same instance");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movingContent_mainComposer
#[test]
fn content_moved_between_parents_keeps_nodes_and_observers() {
    let mut test = composition_test();
    let remembered = Remembered::default();
    let held = remembered.clone();
    let content = movable_content_of(move || {
        let held = held.clone();
        Row!(move || {
            let held = held.clone();
            remember_observer(move || Tracked(held));
            Text("Some text");
            Marker();
        });
    });
    let first = mutable_state_of(true);
    let f = first.handle();
    test.compose(move || {
        let (c1, c2) = (content.clone(), content.clone());
        Row!(move || {
            if f.get() {
                c1();
            }
            Text("Some other text");
        });
        Row!(move || {
            Text("Some more text");
            if !f.get() {
                c2();
            }
        });
    });
    let the_marker = first_named(&test, "Marker");
    let f = first.handle();
    test.validate(move |v| {
        let content = |v: &mut Expected| {
            row(v, |v| {
                v.text("Some text");
                marker(v);
            })
        };
        row(v, |v| {
            if f.get() {
                content(v);
            }
            v.text("Some other text");
        });
        row(v, |v| {
            v.text("Some more text");
            if !f.get() {
                content(v);
            }
        });
    });
    for value in [false, true] {
        first.set(value);
        test.expect_changes();
        test.revalidate();
        assert_eq!(first_named(&test, "Marker"), the_marker, "the marker moves with the content");
        assert!(remembered.is_live(), "the observers stay alive");
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, normalMoveWithContentMove
#[test]
fn keyed_moves_beside_a_content_move() {
    let mut test = composition_test();
    let list: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new((0..10).collect()));
    let scope = ScopeHolder::new();
    let position = mutable_state_of(-1);
    let content = movable_content_of(Marker);
    let (l, s, p) = (list.clone(), scope.clone(), position.handle());
    test.compose(move || {
        s.capture();
        let (l, content) = (l.clone(), content.clone());
        Column!(move || {
            if p.get() == -1 {
                content();
            }
            for item in l.borrow().iter().copied() {
                let content = content.clone();
                key(item, move || {
                    Text(&format!("Item {item}"));
                    if item == p.get() {
                        content();
                    }
                });
            }
        });
    });
    let the_marker = first_named(&test, "Marker");
    let (l, p) = (list.clone(), position.handle());
    test.validate(move |v| {
        column(v, |v| {
            if p.get() == -1 {
                marker(v);
            }
            for item in l.borrow().iter().copied() {
                v.text(&format!("Item {item}"));
                if item == p.get() {
                    marker(v);
                }
            }
        });
    });
    // A shuffle that needs no generator: each step a different order, rotated and reversed.
    for step in 0..=10 {
        position.set(if step == 10 { -1 } else { step });
        {
            let mut list = list.borrow_mut();
            list.rotate_left((step as usize * 3 + 1) % 10);
            if step % 2 == 0 {
                list.reverse();
            }
        }
        scope.invalidate();
        test.expect_changes();
        test.revalidate();
        assert_eq!(first_named(&test, "Marker"), the_marker);
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, removeAndInsertWithMoveAway
#[test]
fn removals_and_insertions_beside_a_content_move() {
    let mut test = composition_test();
    let (position, skip_item) = (mutable_state_of(0), mutable_state_of(5));
    let content = movable_content_of(Marker);
    let (p, s) = (position.handle(), skip_item.handle());
    test.compose(move || {
        let content = content.clone();
        Row!(move || {
            if p.get() == -1 {
                content();
            }
            let content = content.clone();
            Column!(move || {
                for item in 0..10 {
                    let content = content.clone();
                    key(item, move || {
                        if s.get() != item {
                            Text(&format!("Item {item}"));
                        }
                        if p.get() == item {
                            content();
                        }
                    });
                }
            });
        });
    });
    let the_marker = first_named(&test, "Marker");
    let (p, s) = (position.handle(), skip_item.handle());
    test.validate(move |v| {
        row(v, |v| {
            if p.get() == -1 {
                marker(v);
            }
            column(v, |v| {
                for item in 0..10 {
                    if s.get() != item {
                        v.text(&format!("Item {item}"));
                    }
                    if p.get() == item {
                        marker(v);
                    }
                }
            });
        });
    });
    for marker_position in 0..10 {
        for skip in 0..10 {
            let step = |p: i32, s: i32, test: &mut CompositionTest, check: bool| {
                position.set(p);
                skip_item.set(s);
                test.expect_changes();
                if check {
                    test.revalidate();
                    assert_eq!(first_named(test, "Marker"), the_marker);
                }
            };
            step(-1, -1, &mut test, true);
            // Move the marker and delete an item.
            step(marker_position, skip, &mut test, true);
            // Move the marker away and insert an item.
            step(-1, -1, &mut test, false);
            // Move the marker back.
            step(marker_position, -1, &mut test, true);
            // Move the marker away and delete an item.
            step(-1, skip, &mut test, true);
        }
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, invalidationsMoveWithContent
#[test]
fn invalidations_move_with_content() {
    let mut test = composition_test();
    let (data, position) = (mutable_state_of(0), mutable_state_of(-1));
    let d = data.handle();
    let content = movable_content_of(move || Text(&format!("data = {}", d.get())));
    let p = position.handle();
    test.compose(move || {
        let content = content.clone();
        Row!(move || {
            if p.get() == -1 {
                content();
            }
            for item in 0..10 {
                let content = content.clone();
                key(item, move || {
                    Text(&format!("Item {item}"));
                    if p.get() == item {
                        content();
                    }
                });
            }
        });
    });
    let (d, p) = (data.handle(), position.handle());
    test.validate(move |v| {
        row(v, |v| {
            if p.get() == -1 {
                v.text(&format!("data = {}", d.get()));
            }
            for item in 0..10 {
                v.text(&format!("Item {item}"));
                if p.get() == item {
                    v.text(&format!("data = {}", d.get()));
                }
            }
        });
    });
    for value in 0..10 {
        data.set(value);
        position.set(value);
        test.expect_changes();
        test.revalidate();
    }
}

/// A node of `projectedBinaryTree`.
struct TreeNode {
    value: i32,
    left: OwnedState<Option<Rc<TreeNode>>>,
    right: OwnedState<Option<Rc<TreeNode>>>,
    content: RefCell<Option<Callback>>,
}

impl TreeNode {
    fn build(level: i32, index: &mut i32) -> Rc<TreeNode> {
        let value = *index;
        *index += 1;
        let (left, right) = if level > 1 {
            let left = TreeNode::build(level - 1, index);
            (Some(left), Some(TreeNode::build(level - 1, index)))
        } else {
            (None, None)
        };
        Rc::new(TreeNode {
            value,
            left: mutable_state_of_with(left, ReferentialEqualityPolicy),
            right: mutable_state_of_with(right, ReferentialEqualityPolicy),
            content: RefCell::new(None),
        })
    }

    fn for_each(self: &Rc<TreeNode>, block: &mut dyn FnMut(&Rc<TreeNode>)) {
        block(self);
        if let Some(left) = self.left.get() {
            left.for_each(block);
        }
        if let Some(right) = self.right.get() {
            right.for_each(block);
        }
    }

    fn validate(&self, v: &mut Expected) {
        marker_of(v, self.value);
        if let Some(left) = self.left.get() {
            left.validate(v);
        }
        if let Some(right) = self.right.get() {
            right.validate(v);
        }
    }

    fn swap(&self) {
        let (left, right) = (self.left.get(), self.right.get());
        self.left.set(right);
        self.right.set(left);
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, projectedBinaryTree
#[test]
fn a_tree_of_content_swaps_its_children() {
    let mut test = composition_test();
    let tree = TreeNode::build(6, &mut 0);
    let root = tree.clone();
    let mut set_up = |node: &Rc<TreeNode>| {
        let weak = Rc::downgrade(node);
        let content = movable_content_of(move || {
            let node = weak.upgrade().expect("the tree outlives its composition");
            MarkerOf(node.value);
            let call = |child: Option<Rc<TreeNode>>| {
                if let Some(child) = child {
                    let content = child.content.borrow().clone().expect("set up");
                    content();
                }
            };
            call(node.left.get());
            call(node.right.get());
        });
        *node.content.borrow_mut() = Some(content);
    };
    root.for_each(&mut set_up);
    let t = tree.clone();
    test.compose(move || {
        let content = t.content.borrow().clone().expect("set up");
        content();
    });
    let t = tree.clone();
    test.validate(move |v| t.validate(v));
    for _ in 0..2 {
        tree.for_each(&mut |node| node.swap());
        // The tree that `for_each` went through changes under it: what's swapped is all of it.
        test.expect_changes();
        test.revalidate();
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, multipleContentsMovingIntoCommonParent
#[test]
fn several_contents_move_into_a_common_parent() {
    let mut test = composition_test();
    let texts = |a: &'static str, b: &'static str, c: &'static str| {
        movable_content_of(move || {
            Text(a);
            Text(b);
            Text(c);
        })
    };
    let contents =
        [texts("1-1", "1-2", "1-3"), texts("2-4", "2-5", "2-6"), texts("3-7", "3-8", "3-9")];
    let (case, level) = (mutable_state_of(0), mutable_state_of(0));
    #[composable]
    fn Sep() {
        Text("-----");
    }
    fn cases(case: i32, contents: &[Callback; 3]) {
        match case {
            0 => {
                Sep!();
                contents[0]();
                Sep!();
                contents[1]();
                Sep!();
                contents[2]();
                Sep!();
            }
            1 => {
                contents[1]();
                Sep!();
                contents[2]();
                Sep!();
                contents[0]();
            }
            _ => {
                Sep!();
                contents[2]();
                contents[0]();
                contents[1]();
                Sep!();
            }
        }
    }
    let (c, l) = (case.handle(), level.handle());
    let shown = contents.clone();
    test.compose(move || {
        let shown = shown.clone();
        Column!(move || {
            if l.get() == 0 {
                cases(c.get(), &shown);
            }
            let shown = shown.clone();
            Column!(move || {
                if l.get() == 1 {
                    cases(c.get(), &shown);
                }
            });
        });
    });
    let (c, l) = (case.handle(), level.handle());
    test.validate(move |v| {
        let sep = |v: &mut Expected| v.text("-----");
        let value = |v: &mut Expected, at: i32| {
            let texts = [["1-1", "1-2", "1-3"], ["2-4", "2-5", "2-6"], ["3-7", "3-8", "3-9"]];
            for text in texts[at as usize] {
                v.text(text);
            }
        };
        let cases = |v: &mut Expected| match c.get() {
            0 => {
                sep(v);
                value(v, 0);
                sep(v);
                value(v, 1);
                sep(v);
                value(v, 2);
                sep(v);
            }
            1 => {
                value(v, 1);
                sep(v);
                value(v, 2);
                sep(v);
                value(v, 0);
            }
            _ => {
                sep(v);
                value(v, 2);
                value(v, 0);
                value(v, 1);
                sep(v);
            }
        };
        column(v, |v| {
            if l.get() == 0 {
                cases(v);
            }
            column(v, |v| {
                if l.get() == 1 {
                    cases(v);
                }
            });
        });
    });
    let ids = |test: &CompositionTest| {
        ["1-1", "1-2", "1-3", "2-4", "2-5", "2-6", "3-7", "3-8", "3-9"]
            .map(|text| test.find_text_with(text))
    };
    let first = ids(&test);
    for (l, c) in [(0, 0), (1, 1), (0, 2), (1, 0), (0, 1), (1, 2)] {
        case.set(c);
        level.set(l);
        test.advance_ignoring_pending_work();
        test.revalidate();
        assert_eq!(ids(&test), first, "the texts stay the same instances");
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, childIndexesAreCorrectlyCalculated
#[test]
fn a_child_s_index_counts_what_s_before_it() {
    let mut test = composition_test();
    let vertical = mutable_state_of(false);
    let content = movable_content_of(|| MarkerOf(0));
    let w = vertical.handle();
    test.compose(move || {
        let content = content.clone();
        if w.get() {
            Row!(move || {
                Empty!();
                content();
            });
        } else {
            Column!(move || {
                Empty!();
                content();
            });
        }
    });
    let w = vertical.handle();
    test.validate(move |v| {
        if w.get() {
            row(v, |v| marker_of(v, 0));
        } else {
            column(v, |v| marker_of(v, 0));
        }
    });
    vertical.set(true);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, validateRecomposeScopesDoNotGetLost
#[test]
fn recompose_scopes_move_with_content() {
    #[composable]
    fn DisplayInt(value: MutableState<i32>) {
        Text(&format!("value={}", value.get()));
    }
    let mut test = composition_test();
    let (horizontal, display) = (mutable_state_of(false), mutable_state_of(0));
    let d = display.handle();
    let content = movable_content_of(move || DisplayInt!(d));
    let h = horizontal.handle();
    test.compose(move || {
        let content = content.clone();
        let inner = move || {
            let content = content.clone();
            Row!(move || content());
        };
        if h.get() {
            Column!(inner);
        } else {
            Row!(inner);
        }
    });
    let (h, d) = (horizontal.handle(), display.handle());
    test.validate(move |v| {
        let inner = |v: &mut Expected| row(v, |v| v.text(&format!("value={}", d.get())));
        if h.get() {
            column(v, inner);
        } else {
            row(v, inner);
        }
    });
    display.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
    horizontal.set(true);
    test.advance();
    display.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, compositionLocalsShouldBeAvailable
#[test]
fn content_new_each_pass_reads_the_locals_where_it_is() {
    static LOCAL: CompositionLocal<i32> = CompositionLocal::new_static("movable.local", || -1);
    let mut test = composition_test();
    let some_value = mutable_state_of(0);
    let s = some_value.handle();
    test.compose(move || {
        Wrap!(move || {
            CompositionLocalProvider(LOCAL.provides(10), move || {
                // New each time, so new content sees the right providers.
                let content = movable_content_of(move || {
                    Text(&format!("Local = {}", LOCAL.current()));
                    Text(&format!("SomeValue = {}", s.get()));
                });
                if s.get() % 2 == 0 {
                    content();
                } else {
                    content();
                }
            });
        });
    });
    let s = some_value.handle();
    test.validate(move |v| {
        v.text("Local = 10");
        v.text(&format!("SomeValue = {}", s.get()));
    });
    some_value.update(|value| value + 1);
    test.advance();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, compositionLocalsShouldBeAvailableInNestedContent
#[test]
fn nested_content_reads_the_locals_where_it_is() {
    static LOCAL: CompositionLocal<i32> = CompositionLocal::new_static("movable.nested", || -1);
    let mut test = composition_test();
    let some_value = mutable_state_of(0);
    let s = some_value.handle();
    let parent = movable_content_of_with(|child: Callback| Wrap!(move || child()));
    let child = movable_content_of(move || {
        Text(&format!("Local = {}", LOCAL.current()));
        Text(&format!("SomeValue = {}", s.get()));
    });
    let s = some_value.handle();
    test.compose(move || {
        let (parent, child) = (parent.clone(), child.clone());
        Wrap!(move || {
            let (parent, child) = (parent.clone(), child.clone());
            CompositionLocalProvider(LOCAL.provides(10), move || {
                let child = child.clone();
                if s.get() % 2 == 0 {
                    parent(Callback::new(move || {
                        let child = child.clone();
                        Wrap!(move || {
                            Text("One");
                            child();
                        });
                    }));
                } else {
                    parent(Callback::new(move || {
                        child();
                        Text("Two");
                    }));
                }
            });
        });
    });
    let s = some_value.handle();
    test.validate(move |v| {
        let value = s.get();
        if value % 2 == 0 {
            v.text("One");
            v.text("Local = 10");
            v.text(&format!("SomeValue = {value}"));
        } else {
            v.text("Local = 10");
            v.text(&format!("SomeValue = {value}"));
            v.text("Two");
        }
    });
    some_value.update(|value| value + 1);
    test.advance();
    test.revalidate();
}

/// A dynamic local's value at the new place, where the content's reads were subscribed to the old.
#[test]
fn content_moved_under_another_provider_reads_it() {
    static LOCAL: CompositionLocal<i32> = CompositionLocal::new("movable.dynamic", || -1);
    #[composable]
    fn Reader() {
        Text(&format!("Local = {}", LOCAL.current()));
    }
    let mut test = composition_test();
    let (first, value) = (mutable_state_of(true), mutable_state_of(2));
    let content = movable_content_of(|| Reader!());
    let (f, v) = (first.handle(), value.handle());
    test.compose(move || {
        let (a, b) = (content.clone(), content.clone());
        CompositionLocalProvider(LOCAL.provides(1), move || {
            if f.get() {
                a();
            }
        });
        CompositionLocalProvider(LOCAL.provides(v.get()), move || {
            if !f.get() {
                b();
            }
        });
    });
    test.validate(|v| v.text("Local = 1"));
    first.set(false);
    test.expect_changes();
    test.validate(|v| v.text("Local = 2"));
    value.set(3);
    test.expect_changes();
    test.validate(|v| v.text("Local = 3"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentParameters_One
#[test]
fn a_parameter_per_call() {
    let mut test = composition_test();
    let data = mutable_state_of(0);
    let d = data.handle();
    let content =
        movable_content_of_with(move |p1: i32| Text(&format!("Value p1={p1}, data={}", d.get())));
    test.compose(move || {
        content(1);
        content(2);
    });
    let d = data.handle();
    test.validate(move |v| {
        v.text(&format!("Value p1=1, data={}", d.get()));
        v.text(&format!("Value p1=2, data={}", d.get()));
    });
    data.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentParameters_Two
#[test]
fn two_parameters() {
    let mut test = composition_test();
    let data = mutable_state_of(0);
    let d = data.handle();
    let content = movable_content_of_with(move |(p1, p2): (i32, i32)| {
        Text(&format!("Value p1={p1}, p2={p2}, data={}", d.get()));
    });
    test.compose(move || {
        content((1, 2));
        content((3, 4));
    });
    let d = data.handle();
    test.validate(move |v| {
        v.text(&format!("Value p1=1, p2=2, data={}", d.get()));
        v.text(&format!("Value p1=3, p2=4, data={}", d.get()));
    });
    data.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentParameters_Three
#[test]
fn three_parameters() {
    let mut test = composition_test();
    let data = mutable_state_of(0);
    let d = data.handle();
    let content = movable_content_of_with(move |(p1, p2, p3): (i32, i32, i32)| {
        Text(&format!("Value p1={p1}, p2={p2}, p3={p3}, data={}", d.get()));
    });
    test.compose(move || {
        content((1, 2, 3));
        content((4, 5, 6));
    });
    let d = data.handle();
    test.validate(move |v| {
        v.text(&format!("Value p1=1, p2=2, p3=3, data={}", d.get()));
        v.text(&format!("Value p1=4, p2=5, p3=6, data={}", d.get()));
    });
    data.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentParameters_Four
#[test]
fn four_parameters() {
    let mut test = composition_test();
    let data = mutable_state_of(0);
    let d = data.handle();
    let content = movable_content_of_with(move |(p1, p2, p3, p4): (i32, i32, i32, i32)| {
        Text(&format!("Value p1={p1}, p2={p2}, p3={p3}, p4={p4}, data={}", d.get()));
    });
    test.compose(move || {
        content((1, 2, 3, 4));
        content((5, 6, 7, 8));
    });
    let d = data.handle();
    test.validate(move |v| {
        v.text(&format!("Value p1=1, p2=2, p3=3, p4=4, data={}", d.get()));
        v.text(&format!("Value p1=5, p2=6, p3=7, p4=8, data={}", d.get()));
    });
    data.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentReceiver_None
#[test]
fn a_receiver() {
    let mut test = composition_test();
    let data = mutable_state_of(0);
    let d = data.handle();
    let content = movable_content_with_receiver_of(move |receiver: i32, (): ()| {
        Text(&format!("Value this={receiver}, data={}", d.get()));
    });
    test.compose(move || {
        content(100, ());
        content(200, ());
    });
    let d = data.handle();
    test.validate(move |v| {
        v.text(&format!("Value this=100, data={}", d.get()));
        v.text(&format!("Value this=200, data={}", d.get()));
    });
    data.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentReceiver_One
#[test]
fn a_receiver_and_a_parameter() {
    let mut test = composition_test();
    let data = mutable_state_of(0);
    let d = data.handle();
    let content = movable_content_with_receiver_of(move |receiver: i32, p1: i32| {
        Text(&format!("Value this={receiver}, p1={p1}, data={}", d.get()));
    });
    test.compose(move || {
        content(100, 1);
        content(200, 2);
    });
    let d = data.handle();
    test.validate(move |v| {
        v.text(&format!("Value this=100, p1=1, data={}", d.get()));
        v.text(&format!("Value this=200, p1=2, data={}", d.get()));
    });
    data.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentReceiver_Two
#[test]
fn a_receiver_and_two_parameters() {
    let mut test = composition_test();
    let content = movable_content_with_receiver_of(move |receiver: i32, (p1, p2): (i32, i32)| {
        Text(&format!("Value this={receiver}, p1={p1}, p2={p2}"));
    });
    test.compose(move || {
        content(100, (1, 2));
        content(200, (3, 4));
    });
    test.validate(|v| {
        v.text("Value this=100, p1=1, p2=2");
        v.text("Value this=200, p1=3, p2=4");
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentReceiver_Three
#[test]
fn a_receiver_and_three_parameters() {
    let mut test = composition_test();
    let content =
        movable_content_with_receiver_of(move |receiver: i32, (p1, p2, p3): (i32, i32, i32)| {
            Text(&format!("Value this={receiver}, p1={p1}, p2={p2}, p3={p3}"));
        });
    test.compose(move || {
        content(100, (1, 2, 3));
        content(200, (4, 5, 6));
    });
    test.validate(|v| {
        v.text("Value this=100, p1=1, p2=2, p3=3");
        v.text("Value this=200, p1=4, p2=5, p3=6");
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentParameters_changedParameter
#[test]
fn a_parameter_changing_as_content_moves() {
    let mut test = composition_test();
    let (data, location) = (mutable_state_of(0), mutable_state_of(0));
    let content = movable_content_of_with(|d: i32| Text(&format!("d={d}")));
    let (d, l) = (data.handle(), location.handle());
    test.compose(move || {
        let (c1, c2) = (content.clone(), content.clone());
        if l.get() == 0 {
            content(d.get());
        }
        Column!(move || {
            if l.get() == 1 {
                c1(d.get());
            }
        });
        Row!(move || {
            if l.get() == 2 {
                c2(d.get());
            }
        });
    });
    let (d, l) = (data.handle(), location.handle());
    test.validate(move |v| {
        let text = format!("d={}", d.get());
        if l.get() == 0 {
            v.text(&text);
        }
        column(v, |v| {
            if l.get() == 1 {
                v.text(&text);
            }
        });
        row(v, |v| {
            if l.get() == 2 {
                v.text(&text);
            }
        });
    });
    location.update(|value| value + 1);
    data.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
    location.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
    location.set(0);
    data.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentOfTheSameFunctionShouldHaveStableKeys
#[test]
fn content_of_one_function_has_one_hash() {
    let mut test = composition_test();
    let list1: Rc<RefCell<Vec<u64>>> = Rc::default();
    let list2: Rc<RefCell<Vec<u64>>> = Rc::default();
    let (l1, l2) = (list1.clone(), list2.clone());
    let composable1 = move || l1.borrow_mut().push(current_composite_key_hash_code());
    let composable2 = move || l2.borrow_mut().push(current_composite_key_hash_code());
    let (a1, b1) = (movable_content_of(composable1.clone()), movable_content_of(composable1));
    let (a2, b2) = (movable_content_of(composable2.clone()), movable_content_of(composable2));
    test.compose(move || {
        a1();
        b1();
        a1();
        b1();
        a2();
        b2();
        a2();
        b2();
    });
    let (list1, list2) = (list1.borrow(), list2.borrow());
    assert_eq!(list1.len(), 4);
    assert!(list1.iter().all(|hash| *hash == list1[0]));
    assert!(list2.iter().all(|hash| *hash == list2[0]));
    assert_ne!(list1[0], list2[0]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, keyInsideMovableContentShouldntChangeWhenRecomposed
#[test]
fn a_hash_in_content_stays_as_it_recomposes() {
    let mut test = composition_test();
    let hashes: Rc<RefCell<Vec<u64>>> = Rc::default();
    let counter = mutable_state_of(0);
    let (h, c) = (hashes.clone(), counter.handle());
    let content = movable_content_of(move || {
        h.borrow_mut().push(current_composite_key_hash_code());
        Text(&format!("counter={}", c.get()));
    });
    test.compose(move || content());
    let c = counter.handle();
    test.validate(move |v| v.text(&format!("counter={}", c.get())));
    counter.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
    let hashes = hashes.borrow();
    assert_eq!(hashes.len(), 2);
    assert_eq!(hashes[0], hashes[1]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContent_moveRow
#[test]
fn a_row_of_content_replaced_by_the_same() {
    let mut test = composition_test();
    let condition = mutable_state_of(true);
    let first = movable_content_of(|| Text("First"));
    let second = movable_content_of(|| Text("Second"));
    let c = condition.handle();
    test.compose(move || {
        let (first, second) = (first.clone(), second.clone());
        let body = move || {
            let first = first.clone();
            Linear!(move || first());
            second();
        };
        if c.get() {
            Linear!(body);
        } else {
            Linear!(body);
        }
    });
    test.validate(|v| {
        v.linear(|v| {
            v.linear(|v| v.text("First"));
            v.text("Second");
        })
    });
    condition.set(false);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContent_rememberOrdering
#[test]
fn content_removed_beside_a_remember() {
    struct Named;
    impl RememberObserver for Named {
        fn on_remembered(&self) {}
        fn on_forgotten(&self) {}
        fn on_abandoned(&self) {}
    }
    let mut test = composition_test();
    let content = movable_content_of(|| {
        for _ in 0..100 {
            Text("Some content");
        }
    });
    let (include, remember_key) = (mutable_state_of(true), mutable_state_of(0));
    let (i, r) = (include.handle(), remember_key.handle());
    test.compose(move || {
        if i.get() {
            content();
        }
        let key = r.get();
        remember_observer_keyed(key, || Named);
        Text(&format!("Key {key}"));
    });
    remember_key.update(|value| value + 1);
    test.expect_changes();
    include.set(false);
    remember_key.update(|value| value + 1);
    test.expect_changes();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContent_nestedMovableContent
#[test]
fn nested_content_moves_out_and_back() {
    let mut test = composition_test();
    let data = Rc::new(Cell::new(0));
    let condition = mutable_state_of(true);
    let d = data.clone();
    let nested = movable_content_of(move || {
        let d = d.clone();
        let state = remember(move || d.replace(d.get() + 1));
        Text(&format!("Generated state: {state}"));
    });
    let c = condition.handle();
    let inner = nested.clone();
    let host = movable_content_of(move || {
        Text("Host");
        if c.get() {
            inner();
        }
    });
    let c = condition.handle();
    test.compose(move || {
        if c.get() {
            host();
        }
        Text("Outer");
        if !c.get() {
            host();
            nested();
        }
    });
    let c = condition.handle();
    test.validate(move |v| {
        if c.get() {
            v.text("Host");
            v.text("Generated state: 0");
        }
        v.text("Outer");
        if !c.get() {
            v.text("Host");
            v.text("Generated state: 0");
        }
    });
    for value in [false, true] {
        condition.set(value);
        test.expect_changes();
        test.revalidate();
        test.verify_consistent();
    }
}

/// `common` and `wrapper`, for the two that follow.
fn common_and_wrapper() -> (Callback, Callback) {
    let data = Rc::new(Cell::new(0));
    let common = movable_content_of(move || {
        let data = data.clone();
        let state = remember(move || data.replace(data.get() + 1));
        Text(&format!("Generated state: {state}"));
    });
    let inner = common.clone();
    let wrapper = movable_content_of(move || {
        Text("Wrapper start");
        inner();
        Text("Wrapper end");
    });
    (common, wrapper)
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContent_nestedMovableContent_direct
#[test]
fn nested_content_takes_its_wrappers_place() {
    let mut test = composition_test();
    let condition = mutable_state_of(true);
    let (common, wrapper) = common_and_wrapper();
    let c = condition.handle();
    test.compose(move || {
        Text("Outer");
        if c.get() {
            wrapper();
        } else {
            common();
        }
    });
    let c = condition.handle();
    test.validate(move |v| {
        v.text("Outer");
        if c.get() {
            v.text("Wrapper start");
        }
        v.text("Generated state: 0");
        if c.get() {
            v.text("Wrapper end");
        }
    });
    for value in [false, true] {
        condition.set(value);
        test.expect_changes();
        test.revalidate();
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContent_nestedMovableContent_simpleMove
#[test]
fn content_with_nested_content_moves_whole() {
    let mut test = composition_test();
    let condition = mutable_state_of(true);
    let (_common, wrapper) = common_and_wrapper();
    let c = condition.handle();
    test.compose(move || {
        Text("Outer");
        let wrapper = wrapper.clone();
        if c.get() {
            Linear!(move || wrapper());
        } else {
            wrapper();
        }
    });
    let c = condition.handle();
    test.validate(move |v| {
        v.text("Outer");
        let content = |v: &mut Expected| {
            v.text("Wrapper start");
            v.text("Generated state: 0");
            v.text("Wrapper end");
        };
        if c.get() {
            v.linear(content);
        } else {
            content(v);
        }
    });
    for value in [false, true] {
        condition.set(value);
        test.expect_changes();
        test.revalidate();
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContent_nestedMovableContent_tree
#[test]
fn a_tree_of_nested_content_flattens_and_back() {
    let mut test = composition_test();
    let data = Rc::new(Cell::new(0));
    #[composable]
    fn Leaf(data: Rc<Cell<i32>>) {
        let value = remember(move || data.replace(data.get() + 1));
        Text(&format!("Data {value}"));
    }
    let level0: Vec<Callback> = (0..16)
        .map(|_| {
            let data = data.clone();
            movable_content_of(move || Leaf!(data.clone()))
        })
        .collect();
    let up = |below: &[Callback]| -> Vec<Callback> {
        below
            .chunks(2)
            .map(|pair| {
                let (a, b) = (pair[0].clone(), pair[1].clone());
                movable_content_of(move || {
                    a();
                    b();
                })
            })
            .collect()
    };
    let level1 = up(&level0);
    let level2 = up(&level1);
    let level3 = up(&level2);
    let display_tree = mutable_state_of(false);
    let d = display_tree.handle();
    test.compose(move || {
        if d.get() {
            for content in &level3 {
                content();
            }
        } else {
            for content in &level0 {
                content();
            }
        }
    });
    test.validate(|v| {
        for index in 0..16 {
            v.text(&format!("Data {index}"));
        }
    });
    for value in [true, false] {
        display_tree.set(value);
        test.expect_changes();
        test.revalidate();
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContent_movingChildOfDeletedNode
#[test]
fn content_moves_out_of_a_node_going() {
    let mut test = composition_test();
    let index = mutable_state_of(0);
    let content = movable_content_of(|| Text("Some text"));
    let i = index.handle();
    test.compose(move || {
        for _ in i.get()..=3 {
            let content = content.clone();
            Linear!(move || content());
        }
        for _ in 0..=i.get() {
            let content = content.clone();
            Linear!(move || content());
        }
    });
    let i = index.handle();
    test.validate(move |v| {
        for _ in 0..5 {
            v.linear(|v| v.text("Some text"));
        }
        let _ = i;
    });
    for _ in 0..3 {
        index.update(|value| value + 1);
        test.advance();
        test.revalidate();
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentReactivated
#[test]
fn content_deactivated_comes_back_in_another_branch() {
    let value = mutable_state_of(true);
    let text = mutable_state_of(String::from("text"));
    let t = text.handle();
    let movable = movable_content_of(move || {
        Linear!(move || Text(&t.get()));
    });
    let v = value.handle();
    let content = move || {
        // Two groups, as the two branches of Compose's `if`.
        let movable = movable.clone();
        key(v.get(), move || movable());
    };
    let composition =
        Composition::reusable(ViewApplier::new(), Recomposer::current_thread().as_context());
    composition.set_content(content.clone());
    composition.deactivate();
    value.set(false);
    composition.set_content_with_reuse(content.clone());
    composition.deactivate();
    value.set(true);
    text.set("new text".into());
    composition.set_content_with_reuse(content);
    let shown = composition.with_applier(|views: &mut ViewApplier| views.tree.dump());
    assert_eq!(shown.trim(), "<linear>\n  <text text='new text'>");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentInvalidatedWhileDeleted_gapComposer
#[test]
fn content_invalidated_as_it_s_removed() {
    let mut test = composition_test();
    let value = mutable_state_of(true);
    let target: Rc<RefCell<Option<RecomposeScope>>> = Rc::default();
    let t = target.clone();
    let movable = movable_content_of_with(move |key: i32| {
        let t = t.clone();
        Linear!(move || {
            *t.borrow_mut() = Some(current_recompose_scope());
            Text(&key.to_string());
        });
    });
    let (v, t) = (value.handle(), target.clone());
    test.compose(move || {
        if v.get() {
            movable(20);
        } else {
            Linear!(|| {
                for index in 0..5 {
                    Text(&index.to_string());
                }
            });
        }
        if let Some(scope) = t.borrow().as_ref() {
            scope.invalidate();
        }
    });
    test.validate(|v| v.linear(|v| v.text("20")));
    value.set(false);
    test.advance_ignoring_pending_work();
    test.validate(|v| {
        v.linear(|v| {
            for index in 0..5 {
                v.text(&index.to_string());
            }
        })
    });
    test.verify_consistent();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentRemovedFromWriter
#[test]
fn content_removed_after_a_removal_before_it() {
    let mut test = composition_test();
    let value = mutable_state_of(true);
    let movable = movable_content_of_with(|key: i32| {
        Linear!(move || Text(&key.to_string()));
    });
    let v = value.handle();
    test.compose(move || {
        if v.get() {
            Text("Hello");
        }
        if v.get() {
            movable(0);
        }
    });
    test.validate(|v| {
        v.text("Hello");
        v.linear(|v| v.text("0"));
    });
    value.set(false);
    test.advance();
    test.validate(|_| {});
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, moveNestedMovableContentFromNodeGroup
#[test]
fn nested_content_moves_out_of_a_node_of_content_removed() {
    let mut test = composition_test();
    let (move_nested, show_outer) = (mutable_state_of(false), mutable_state_of(true));
    let inner = movable_content_of(|| {
        for index in 0..10 {
            Text(&index.to_string());
        }
    });
    let nested = inner.clone();
    let outer = movable_content_of(move || {
        let nested = nested.clone();
        Linear!(move || {
            Text("Hello");
            nested();
        });
    });
    let (m, s) = (move_nested.handle(), show_outer.handle());
    test.compose(move || {
        let (inner, outer) = (inner.clone(), outer.clone());
        Linear!(move || {
            if s.get() {
                outer();
            }
            if m.get() {
                inner();
            }
        });
    });
    test.validate(|v| {
        v.linear(|v| {
            v.linear(|v| {
                v.text("Hello");
                for index in 0..10 {
                    v.text(&index.to_string());
                }
            })
        })
    });
    show_outer.set(false);
    move_nested.set(true);
    test.expect_changes();
    test.validate(|v| {
        v.linear(|v| {
            for index in 0..10 {
                v.text(&index.to_string());
            }
        })
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, testUpdateMovingNestedContent
#[test]
fn nested_content_moving_with_a_new_parameter() {
    let mut test = composition_test();
    let (move_content, value) = (mutable_state_of(false), mutable_state_of(1));
    let nested = movable_content_of_with(|param: i32| {
        Linear!(move || {
            key(1, || {
                remember_keyed(param, || param);
            });
        });
    });
    let (m, v, n) = (move_content.handle(), value.handle(), nested.clone());
    let parent = movable_content_of(move || {
        let n = n.clone();
        Linear!(move || {
            if !m.get() {
                n(v.get());
            }
        });
    });
    let (m, v) = (move_content.handle(), value.handle());
    test.compose(move || {
        let (parent, nested) = (parent.clone(), nested.clone());
        Linear!(move || {
            let parent = parent.clone();
            key("A", || Linear!(move || parent()));
            let nested = nested.clone();
            key("B", || {
                Linear!(move || {
                    if m.get() {
                        nested(v.get());
                    }
                })
            });
        });
    });
    value.update(|value| value + 1);
    move_content.set(true);
    test.expect_changes();
    value.update(|value| value + 1);
    move_content.set(false);
    test.expect_changes();
    test.verify_consistent();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentDeletingInnerInvalidatedScopeClearsInvalidations
#[test]
fn content_moved_as_an_invalid_scope_in_it_goes() {
    let mut test = composition_test();
    let (moved, show_inner, inner_state) =
        (mutable_state_of(false), mutable_state_of(true), mutable_state_of(0));
    let (s, i) = (show_inner.handle(), inner_state.handle());
    let content = movable_content_of(move || {
        if s.get() {
            Wrap!(move || Text(&format!("Inner: {}", i.get())));
        }
    });
    let m = moved.handle();
    test.compose(move || {
        let content = content.clone();
        if !m.get() {
            Column!(move || content());
        } else {
            Row!(move || content());
        }
    });
    test.validate(|v| column(v, |v| v.text("Inner: 0")));
    moved.set(true);
    show_inner.set(false);
    inner_state.set(1);
    test.expect_changes();
    test.validate(|v| row(v, |_| {}));
    test.verify_consistent();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt, movableContentInvalidatingMainContentDuringMove
#[test]
fn content_moving_invalidates_what_composed_before_it() {
    #[composable]
    fn MainContent(text: Rc<RefCell<String>>, scope: Rc<RefCell<Option<RecomposeScope>>>) {
        *scope.borrow_mut() = Some(current_recompose_scope());
        Text(&format!("Main: {}", text.borrow()));
    }
    let mut test = composition_test();
    let (moved, trigger) = (mutable_state_of(false), mutable_state_of(0));
    let main_text = Rc::new(RefCell::new(String::from("Initial")));
    let main_scope: Rc<RefCell<Option<RecomposeScope>>> = Rc::default();
    let (m, text, scope) = (moved.handle(), main_text.clone(), main_scope.clone());
    let content = movable_content_of(move || {
        if m.get() {
            *text.borrow_mut() = "UpdatedDuringMove".into();
            if let Some(scope) = scope.borrow().as_ref() {
                scope.invalidate();
            }
        }
        Text("Movable");
    });
    let (m, t) = (moved.handle(), trigger.handle());
    test.compose(move || {
        Text(&format!("Trigger: {}", t.get()));
        let (c1, c2) = (content.clone(), content.clone());
        if !m.get() {
            Column!(move || c1());
        }
        MainContent!(main_text.clone(), main_scope.clone());
        if m.get() {
            Row!(move || c2());
        }
    });
    test.validate(|v| {
        v.text("Trigger: 0");
        column(v, |v| v.text("Movable"));
        v.text("Main: Initial");
    });
    moved.set(true);
    test.expect_changes();
    trigger.set(1);
    test.expect_changes();
    test.validate(|v| {
        v.text("Trigger: 1");
        v.text("Main: UpdatedDuringMove");
        row(v, |v| v.text("Movable"));
    });
    test.verify_consistent();
}

/// What's removed for good is released, its observers forgotten, once it's known nothing takes it.
#[test]
fn content_removed_with_no_insertion_goes_at_the_end_of_the_pass() {
    let mut test = composition_test();
    let there = mutable_state_of(true);
    let remembered = Remembered::default();
    let held = remembered.clone();
    let content = movable_content_of(move || {
        let held = held.clone();
        remember_observer(move || Tracked(held));
        Text("Some text");
    });
    let t = there.handle();
    test.compose(move || {
        if t.get() {
            content();
        }
    });
    assert!(remembered.is_live());
    there.set(false);
    test.expect_changes();
    test.validate(|_| {});
    assert!(!remembered.is_live());
}

/// Content under a node that goes, with nothing to take it, goes with the node.
#[test]
fn content_under_a_node_going_goes_when_nothing_takes_it() {
    let mut test = composition_test();
    let there = mutable_state_of(true);
    let remembered = Remembered::default();
    let held = remembered.clone();
    let content = movable_content_of(move || {
        let held = held.clone();
        remember_observer(move || Tracked(held));
        Text("Some text");
    });
    let t = there.handle();
    test.compose(move || {
        if t.get() {
            let content = content.clone();
            Linear!(move || content());
        }
    });
    there.set(false);
    test.expect_changes();
    test.validate(|_| {});
    assert!(!remembered.is_live());
    test.verify_consistent();
}

/// Two of one content going, one coming: it takes the last that went, as `removeLast`.
#[test]
fn the_last_content_removed_is_the_one_taken() {
    let mut test = composition_test();
    let next = Rc::new(Cell::new(0));
    let n = next.clone();
    let content = movable_content_of(move || {
        let n = n.clone();
        let token = remember(move || n.replace(n.get() + 1));
        Text(&format!("token {token}"));
    });
    let place = mutable_state_of(0);
    let p = place.handle();
    test.compose(move || {
        if p.get() == 0 {
            let (a, b) = (content.clone(), content.clone());
            Linear!(move || a());
            Row!(move || b());
        } else {
            let a = content.clone();
            Column!(move || a());
        }
    });
    test.validate(|v| {
        v.linear(|v| v.text("token 0"));
        row(v, |v| v.text("token 1"));
    });
    place.set(1);
    test.expect_changes();
    test.validate(|v| column(v, |v| v.text("token 1")));
}

/// Two contents among other nodes of a node going: each comes out with its own nodes.
#[test]
fn contents_come_out_of_a_node_going_from_among_its_nodes() {
    #[composable(non_restartable)]
    fn Inner(a: Callback, b: Callback) {
        Text("zero");
        a();
        Linear!(|| Text("one"));
        b();
        Text("two");
    }
    let mut test = composition_test();
    let first = mutable_state_of(true);
    let a = movable_content_of(|| {
        Text("a1");
        Text("a2");
    });
    let b = movable_content_of(|| Text("b"));
    let f = first.handle();
    test.compose(move || {
        let (a, b) = (a.clone(), b.clone());
        if f.get() {
            Row!(move || Inner(a.clone(), b.clone()));
        } else {
            Column!(move || {
                b();
                a();
            });
        }
    });
    test.validate(|v| {
        row(v, |v| {
            v.text("zero");
            v.text("a1");
            v.text("a2");
            v.linear(|v| v.text("one"));
            v.text("b");
            v.text("two");
        })
    });
    let ids = ["a1", "a2", "b"].map(|text| test.find_text_with(text));
    first.set(false);
    test.expect_changes();
    test.validate(|v| {
        column(v, |v| {
            v.text("b");
            v.text("a1");
            v.text("a2");
        })
    });
    assert_eq!(["a1", "a2", "b"].map(|text| test.find_text_with(text)), ids);
}

/// Content after a node holding other content, in a node going: its index counts that node.
#[test]
fn content_comes_out_from_after_a_node_with_content() {
    let mut test = composition_test();
    let first = mutable_state_of(true);
    let (a, b) = (movable_content_of(|| Text("a")), movable_content_of(|| Text("b")));
    let f = first.handle();
    test.compose(move || {
        let (a, b) = (a.clone(), b.clone());
        if f.get() {
            Row!(move || {
                let a = a.clone();
                Linear!(move || a());
                b();
            });
        } else {
            Column!(move || {
                a();
                b();
            });
        }
    });
    let ids = ["a", "b"].map(|text| test.find_text_with(text));
    first.set(false);
    test.expect_changes();
    test.validate(|v| {
        column(v, |v| {
            v.text("a");
            v.text("b");
        })
    });
    assert_eq!(["a", "b"].map(|text| test.find_text_with(text)), ids);
}

/// A content leaves its wrapper as the wrapper moves too: each with its own nodes.
#[test]
fn nested_content_leaves_its_wrapper_moving_too() {
    let mut test = composition_test();
    let moved = mutable_state_of(false);
    let (m, inner) = (moved.handle(), movable_content_of(|| Text("inner")));
    let nested = inner.clone();
    let wrapper = movable_content_of(move || {
        Text("start");
        if !m.get() {
            nested();
        }
        Text("end");
    });
    let m = moved.handle();
    test.compose(move || {
        let (inner, wrapper) = (inner.clone(), wrapper.clone());
        if m.get() {
            Row!(move || inner());
            Column!(move || wrapper());
        } else {
            Linear!(move || wrapper());
        }
    });
    test.validate(|v| {
        v.linear(|v| {
            v.text("start");
            v.text("inner");
            v.text("end");
        })
    });
    let id = test.find_text_with("inner");
    moved.set(true);
    test.expect_changes();
    test.validate(|v| {
        row(v, |v| v.text("inner"));
        column(v, |v| {
            v.text("start");
            v.text("end");
        });
    });
    assert_eq!(test.find_text_with("inner"), id);
    test.verify_consistent();
}

// ---- From the other files -----------------------------------------------------------------

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onReleaseIsNotCalledWithMovableContentMovement
#[test]
fn a_node_moved_out_of_reusable_content_isn_t_released() {
    let mut test = composition_test();
    let wrap = mutable_state_of(true);
    let released = Rc::new(Cell::new(0));
    let r = released.clone();
    let content = movable_content_of(move || {
        let r = r.clone();
        let on_release: Action = Rc::new(move || r.set(r.get() + 1));
        LinearWithCallbacks!(nothing(), nothing(), on_release, nothing(), || {});
    });
    let w = wrap.handle();
    test.compose(move || {
        if w.get() {
            let content = content.clone();
            ReusableContent(0, move || content());
        } else {
            content();
        }
    });
    wrap.set(false);
    test.expect_changes();
    assert_eq!(released.get(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, reusableContentHost_movableContent
#[test]
fn a_child_with_content_goes_on_after_its_place_went() {
    let mut test = composition_test();
    let (active, state) = (mutable_state_of(true), mutable_state_of(0));
    let child: Rc<RefCell<Option<Composition>>> = Rc::default();
    let s = state.handle();
    let content = movable_content_of(move || Text(&format!("State: {}", s.get())));
    let (a, c) = (active.handle(), child.clone());
    test.compose(move || {
        if a.get() {
            let context = remember_composition_context();
            let mut held = c.borrow_mut();
            if held.is_none() {
                let composition = platform::subcomposition(context, ViewApplier::new().root());
                let content = content.clone();
                composition.set_content(move || content());
                *held = Some(composition);
            }
        }
    });
    state.update(|value| value + 1);
    active.set(false);
    test.advance();
    // Detaching it leaves nothing it can't compose on from.
    child.borrow().as_ref().expect("composed").set_content(|| Text("This is a test"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, movableContentNoopInDeactivatedComposition
#[test]
fn content_in_a_composition_deactivated_in_a_frame() {
    let mut test = composition_test();
    let state = mutable_state_of(false);
    let content = movable_content_of(|| Text("Test"));
    let context: Rc<Cell<Option<CompositionContext>>> = Rc::default();
    let child: Rc<RefCell<Option<ReusableComposition>>> = Rc::default();
    let (s, c, h) = (state.handle(), context.clone(), child.clone());
    test.compose(move || {
        c.set(Some(remember_composition_context()));
        let value = s.get();
        let h = h.clone();
        SideEffect(move || {
            if value && let Some(child) = h.borrow().as_ref() {
                child.deactivate();
            }
        });
    });
    let composition = platform::reusable_subcomposition(
        context.get().expect("composed"),
        ViewApplier::new().root(),
    );
    let s = state.handle();
    composition.set_content(move || {
        if s.get() {
            content();
        }
    });
    *child.borrow_mut() = Some(composition);
    state.set(true);
    test.advance();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerFrameEndSchedulingTests.kt, scheduleFrameEnd_Callback_invokesAfterMovableContentSettles
#[test]
fn a_frame_end_callback_waits_for_content_to_settle() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let include = mutable_state_of(true);
    let settled = Rc::new(Cell::new(false));
    let resumed = Rc::new(Cell::new(false));
    let s = settled.clone();
    let content = movable_content_of(move || {
        let s = s.clone();
        DisposableEffect((), move |scope| scope.on_dispose(move || s.set(true)));
    });
    let (i, s, r) = (include.handle(), settled.clone(), resumed.clone());
    composition.set_content(move || {
        if i.get() {
            content();
        } else {
            let (s, r) = (s.clone(), r.clone());
            SideEffect(move || {
                let (s, r) = (s.clone(), r.clone());
                recomposer.schedule_frame_end_callback(move || {
                    assert!(s.get(), "it ran before the content settled");
                    r.set(true);
                });
            });
        }
    });
    include.set(false);
    assert!(platform::perform_frame(recomposer, 0));
    assert!(!recomposer.has_pending_work());
    assert!(resumed.get());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerFrameEndSchedulingTests.kt, scheduleFrameCallback
#[test]
fn a_frame_end_callback_of_a_context_waits_for_content_to_settle() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let include = mutable_state_of(true);
    let settled = Rc::new(Cell::new(false));
    let s = settled.clone();
    let content = movable_content_of(move || {
        let s = s.clone();
        DisposableEffect((), move |scope| scope.on_dispose(move || s.set(true)));
    });
    let i = include.handle();
    composition.set_content(move || {
        if i.get() {
            content();
        }
    });
    include.set(false);
    let resumed = Rc::new(Cell::new(false));
    let (s, r) = (settled.clone(), resumed.clone());
    composition.parent().schedule_frame_end_callback(move || {
        assert!(s.get(), "it ran before the content settled");
        r.set(true);
    });
    assert!(!resumed.get());
    assert!(platform::perform_frame(recomposer, 0));
    assert!(!recomposer.has_pending_work());
    assert!(resumed.get());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, onlyInvalidatesIfResultIsDifferentInMovableContent
#[test]
fn a_derived_value_unchanged_doesn_t_invalidate_content() {
    let mut test = composition_test();
    let (a, b) = (mutable_state_of(32), mutable_state_of(10));
    let (ra, rb) = (a.handle(), b.handle());
    let answer = derived_state_of(move || ra.get() + rb.get());
    let answer = remember_free(answer);
    let count = Rc::new(Cell::new(0));
    let c = count.clone();
    let content = movable_content_of(move || {
        c.set(c.get() + 1);
        Text(&format!("The answer is {}", answer.get()));
    });
    test.compose(move || content());
    let (ra, rb) = (a.handle(), b.handle());
    test.validate(move |v| v.text(&format!("The answer is {}", ra.get() + rb.get())));
    assert_eq!(count.get(), 1);
    compose_runtime::snapshots::Snapshot::with_mutable_snapshot(|| {
        a.update(|value| value + 1);
        b.update(|value| value - 1);
    });
    test.expect_no_changes();
    test.revalidate();
    assert_eq!(count.get(), 1);
    a.update(|value| value + 1);
    test.expect_changes();
    test.revalidate();
    assert_eq!(count.get(), 2);
}

/// A handle that outlives the test's scope, as the Kotlin `val answer by derivedStateOf` does.
fn remember_free<T>(state: T) -> &'static T {
    Box::leak(Box::new(state))
}

// ---- A differential fuzz ------------------------------------------------------------------

const CONTENTS: usize = 7;
const PARENTS: usize = 3;

/// Where each content is: in a parent, or in a content before it, or nowhere.
type Placing = Vec<Vec<usize>>;

struct Fuzz {
    placing: OwnedState<Rc<Placing>>,
    contents: RefCell<Vec<Callback>>,
    tokens: RefCell<Vec<Option<u64>>>,
    next: Cell<u64>,
    live: Cell<i32>,
}

struct Alive(Rc<Fuzz>);

impl RememberObserver for Alive {
    fn on_remembered(&self) {
        self.0.live.set(self.0.live.get() + 1);
    }
    fn on_forgotten(&self) {
        self.0.live.set(self.0.live.get() - 1);
    }
    fn on_abandoned(&self) {}
}

fn compose_hosted(fuzz: &Rc<Fuzz>, host: usize) {
    let placing = fuzz.placing.get();
    for content in &placing[host] {
        let call = fuzz.contents.borrow()[*content].clone();
        call();
    }
}

fn expected_hosted(fuzz: &Fuzz, placing: &Placing, host: usize, v: &mut Expected) {
    for content in &placing[host] {
        let token = fuzz.tokens.borrow()[*content].expect("composed");
        v.linear(|v| {
            v.text(&format!("{content}:{token}"));
            expected_hosted(fuzz, placing, PARENTS + content, v);
        });
    }
}

fn reached(placing: &Placing, host: usize, out: &mut Vec<usize>) {
    for content in &placing[host] {
        out.push(*content);
        reached(placing, PARENTS + content, out);
    }
}

fn run_fuzz(seed: u64) {
    let mut random = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let mut next = move |bound: u64| {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        random % bound
    };
    let fuzz = Rc::new(Fuzz {
        placing: mutable_state_of_with(
            Rc::new(vec![Vec::new(); PARENTS + CONTENTS]),
            ReferentialEqualityPolicy,
        ),
        contents: RefCell::new(Vec::new()),
        tokens: RefCell::new(vec![None; CONTENTS]),
        next: Cell::new(1),
        live: Cell::new(0),
    });
    for index in 0..CONTENTS {
        let inner = Rc::downgrade(&fuzz);
        let content = movable_content_of(move || {
            let fuzz = inner.upgrade().expect("it outlives the composition");
            let made = fuzz.clone();
            let token = remember(move || {
                let token = made.next.get();
                made.next.set(token + 1);
                token
            });
            let alive = fuzz.clone();
            remember_observer(move || Alive(alive));
            fuzz.tokens.borrow_mut()[index] = Some(token);
            let inner = inner.clone();
            Linear!(move || {
                let fuzz = inner.upgrade().expect("it outlives the composition");
                Text(&format!("{index}:{token}"));
                compose_hosted(&fuzz, PARENTS + index);
            });
        });
        fuzz.contents.borrow_mut().push(content);
    }
    let mut test = composition_test();
    let f = fuzz.clone();
    test.compose(move || {
        let (a, b, c) = (f.clone(), f.clone(), f.clone());
        Row!(move || compose_hosted(&a, 0));
        Column!(move || {
            compose_hosted(&b, 1);
            let c = c.clone();
            Row!(move || compose_hosted(&c, 2));
        });
    });
    let mut last_tokens: Vec<Option<u64>> = vec![None; CONTENTS];
    for _ in 0..25 {
        let mut placing: Placing = vec![Vec::new(); PARENTS + CONTENTS];
        for content in 0..CONTENTS {
            if next(4) == 0 {
                continue;
            }
            // In a parent, or in a content before it, keeping the tree a tree.
            let hosts = (PARENTS + content) as u64;
            let host = next(hosts) as usize;
            let at = next(placing[host].len() as u64 + 1) as usize;
            placing[host].insert(at, content);
        }
        let mut present = Vec::new();
        for parent in 0..PARENTS {
            reached(&placing, parent, &mut present);
        }
        fuzz.placing.set(Rc::new(placing));
        test.advance();
        test.verify_consistent();

        // Each content that stayed kept what it remembered; what's new has its own.
        let placing = fuzz.placing.get();
        let tokens = fuzz.tokens.borrow().clone();
        for content in 0..CONTENTS {
            if present.contains(&content)
                && let Some(last) = last_tokens[content]
            {
                assert_eq!(tokens[content], Some(last), "{content} of {placing:?}, seed {seed}");
            }
            last_tokens[content] = if present.contains(&content) { tokens[content] } else { None };
        }
        assert_eq!(fuzz.live.get(), present.len() as i32, "{placing:?}, seed {seed}");
        let (f, p) = (fuzz.clone(), placing.clone());
        test.validate(move |v| {
            row(v, |v| expected_hosted(&f, &p, 0, v));
            column(v, |v| {
                expected_hosted(&f, &p, 1, v);
                row(v, |v| expected_hosted(&f, &p, 2, v));
            });
        });
    }
    test.dispose();
    assert_eq!(fuzz.live.get(), 0, "seed {seed}");
}

/// Contents placed at random, in parents and in each other, against plain arrays.
#[test]
fn contents_placed_at_random_agree_with_arrays() {
    for seed in 1..=120 {
        run_fuzz(seed);
    }
}
