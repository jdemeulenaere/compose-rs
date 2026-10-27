//! Runtime scenarios, against composables written by hand the way `#[composable]` expands them
//! (docs/03-composable-macro.md): a restart group, kept arguments compared with `ComposeArg`,
//! `skipping()`/`skip_to_group_end()`, and a non-capturing restart function.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::platform::*;
use compose_runtime::*;

// ---- Test support --------------------------------------------------------------------------

/// A node label that dumps without quotes.
#[derive(Clone, PartialEq)]
struct Label(String);

impl fmt::Debug for Label {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

type Tree = NodeTree<Label>;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static SERIAL: Cell<u32> = const { Cell::new(0) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

fn next_serial() -> u32 {
    SERIAL.with(|serial| {
        let value = serial.get();
        serial.set(value + 1);
        value
    })
}

fn new_composition() -> Composition {
    take_log();
    SERIAL.with(|serial| serial.set(0));
    Composition::new(Tree::new())
}

fn tree(composition: &Composition) -> String {
    composition.with_applier(|tree: &mut Tree| tree.dump())
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => payload.downcast_ref::<&str>().map(|s| s.to_string()).unwrap_or_default(),
    }
}

/// Logs when created and dropped.
struct Token {
    name: String,
}

impl Token {
    fn new(name: impl Into<String>) -> Token {
        let name = name.into();
        log(format!("create {name}"));
        Token { name }
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        log(format!("drop {}", self.name));
    }
}

// ---- Hand-expanded composables -------------------------------------------------------------

/// A non-restartable composable emitting one leaf node.
fn Leaf(label: &str) {
    let composer = current();
    let node = composer.start_node(key_of("scenarios::Leaf"), || Label(label.to_string()));
    composer.with_applier(|tree: &mut Tree| {
        let value = tree.get_mut(node).expect("live node");
        if value.0 != label {
            value.0 = label.to_string();
        }
    });
    composer.end_group();
}

/// `#[composable] fn Text(text: String) { Leaf(&text) }`
fn Text(text: String) {
    let composer = current();
    composer.start_restart_group(key_of("scenarios::Text"));
    let changed = composer.update_kept(|kept: &(String,)| !kept.0.same(&text), || (text.clone(),));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        log(format!("Text({text})"));
        Leaf(&text);
    }
    composer.end_restart_group(|kept| {
        let (text,) = kept.downcast_ref::<(String,)>().expect("kept arguments").clone();
        Text(text)
    });
}

/// `#[composable] fn Reader(name: &'static str, state: MutableState<i32>)`: shows the state.
fn Reader(name: &'static str, state: MutableState<i32>) {
    let composer = current();
    composer.start_restart_group(key_of("scenarios::Reader"));
    let changed = composer.update_kept(
        |kept: &(&'static str, MutableState<i32>)| !kept.0.same(&name) || !kept.1.same(&state),
        || (name, state),
    );
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        log(name);
        Leaf(&format!("{name}={}", state.get()));
    }
    composer.end_restart_group(|kept| {
        let (name, state) =
            *kept.downcast_ref::<(&'static str, MutableState<i32>)>().expect("kept");
        Reader(name, state)
    });
}

/// A restartable composable whose body is a plain function of its arguments: the expansion's
/// shape, with the body passed in, so tests can declare composables in one line.
fn restartable<K: ComposeArg + Clone>(key: u64, args: K, body: fn(K)) {
    let composer = current();
    composer.start_restart_group(key);
    let changed = composer
        .update_kept(|kept: &(u64, K, fn(K))| !kept.1.same(&args), || (key, args.clone(), body));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        body(args);
    }
    composer.end_restart_group(|kept| {
        let (key, args, body) = kept.downcast_ref::<(u64, K, fn(K))>().expect("kept").clone();
        restartable(key, args, body)
    });
}

/// A region group around `content`, as the macro emits for a branch or a loop.
fn region<R>(key: u64, content: impl FnOnce() -> R) -> R {
    let composer = current();
    composer.start_replace_group(key);
    let result = content();
    composer.end_group();
    result
}

// ---- Scenarios -----------------------------------------------------------------------------

#[test]
fn initial_composition_emits_nodes_in_order() {
    let composition = new_composition();
    composition.set_content(|| {
        Text("a".into());
        Text("b".into());
        Leaf("c");
    });
    assert_eq!(tree(&composition), "a\nb\nc\n");
    assert_eq!(take_log(), ["Text(a)", "Text(b)"]);
    assert!(!composition.has_invalidations());
    assert!(!composition.recompose());
    composition.validate();
}

#[test]
fn writing_state_recomposes_only_its_readers() {
    let composition = new_composition();
    let a = mutable_state_of(0);
    let b = mutable_state_of(0);
    let (a_handle, b_handle) = (a.handle(), b.handle());
    composition.set_content(move || {
        log("content");
        Reader("a", a_handle);
        Reader("b", b_handle);
    });
    assert_eq!(take_log(), ["content", "a", "b"]);

    a.set(1);
    assert!(composition.has_invalidations());
    assert!(composition.recompose());
    assert_eq!(take_log(), ["a"]);
    assert_eq!(tree(&composition), "a=1\nb=0\n");
    assert!(!composition.recompose());

    // An equal value invalidates nothing (structural equality policy).
    b.set(0);
    assert!(!composition.has_invalidations());
    b.set(5);
    composition.recompose();
    assert_eq!(take_log(), ["b"]);
    assert_eq!(tree(&composition), "a=1\nb=5\n");
    composition.validate();
}

#[test]
fn unchanged_arguments_skip_and_changed_ones_recompose() {
    let composition = new_composition();
    let x = mutable_state_of(0);
    let handle = x.handle();
    composition.set_content(move || {
        restartable(key_of("Parent"), handle, |x| {
            log("Parent");
            Text("constant".into());
            Text(format!("x={}", x.get()));
        })
    });
    assert_eq!(take_log(), ["Parent", "Text(constant)", "Text(x=0)"]);
    x.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["Parent", "Text(x=1)"]);
    assert_eq!(tree(&composition), "constant\nx=1\n");
    composition.validate();
}

#[test]
fn a_scope_recomposes_alone_below_a_skipped_parent_and_reuses_its_group() {
    let composition = new_composition();
    let state = mutable_state_of(0);
    let handle = state.handle();
    composition.set_content(move || {
        restartable(key_of("Wrapper"), handle, |state| {
            log("Wrapper");
            Leaf("before");
            Reader("r", state);
            Leaf("after");
        })
    });
    take_log();
    let groups = composition.group_count();
    for value in 1..=10 {
        state.set(value);
        composition.recompose();
        assert_eq!(take_log(), ["r"]);
        assert_eq!(composition.group_count(), groups, "the restart reused its group");
        composition.validate();
    }
    assert_eq!(tree(&composition), "before\nr=10\nafter\n");
}

#[test]
fn branches_in_region_groups_keep_their_state_apart() {
    let composition = new_composition();
    let flag = mutable_state_of(true);
    let handle = flag.handle();
    composition.set_content(move || {
        restartable(key_of("Holder"), handle, |flag| {
            if flag.get() {
                region(1, || {
                    let token = remember(|| Rc::new(Token::new(format!("on{}", next_serial()))));
                    Leaf(&token.name);
                });
            } else {
                region(2, || Leaf("off"));
            }
        })
    });
    assert_eq!(tree(&composition), "on0\n");
    flag.set(false);
    composition.recompose();
    assert_eq!(tree(&composition), "off\n");
    flag.set(true);
    composition.recompose();
    assert_eq!(tree(&composition), "on1\n");
    assert_eq!(take_log(), ["create on0", "drop on0", "create on1"]);
    composition.validate();
}

/// `#[composable] fn Row(id: u32)`: remembers a serial number when first composed.
fn Row(id: u32) {
    restartable(key_of("Row"), id, |id| {
        log(format!("Row({id})"));
        let serial = remember(next_serial);
        Leaf(&format!("{id}:t{serial}"));
    })
}

#[test]
fn keyed_items_keep_their_state_when_moved() {
    let composition = new_composition();
    let list = mutable_state_of(vec![1u32, 2, 3]);
    let handle = list.handle();
    composition.set_content(move || {
        restartable(key_of("Column"), handle, |list| {
            list.with(|ids| {
                for &id in ids {
                    key(id, || Row(id));
                }
            })
        })
    });
    assert_eq!(tree(&composition), "1:t0\n2:t1\n3:t2\n");
    take_log();

    list.set(vec![3, 1, 2]);
    composition.recompose();
    assert_eq!(tree(&composition), "3:t2\n1:t0\n2:t1\n");
    assert!(take_log().is_empty(), "moved rows skip");

    list.set(vec![3, 4, 1, 2]);
    composition.recompose();
    assert_eq!(tree(&composition), "3:t2\n4:t3\n1:t0\n2:t1\n");
    assert_eq!(take_log(), ["Row(4)"]);

    list.set(vec![2, 4]);
    composition.recompose();
    assert_eq!(tree(&composition), "2:t1\n4:t3\n");
    assert!(take_log().is_empty());

    list.set(vec![4, 2, 9, 1]);
    composition.recompose();
    assert_eq!(tree(&composition), "4:t3\n2:t1\n9:t4\n1:t5\n");
    composition.validate();
}

#[test]
fn unkeyed_items_match_by_position() {
    let composition = new_composition();
    let list = mutable_state_of(vec![1u32, 2, 3]);
    let handle = list.handle();
    composition.set_content(move || {
        restartable(key_of("Column"), handle, |list| {
            list.with(|ids| ids.iter().for_each(|&id| Row(id)))
        })
    });
    assert_eq!(tree(&composition), "1:t0\n2:t1\n3:t2\n");
    take_log();
    list.set(vec![0, 1, 2, 3]);
    composition.recompose();
    // Positional memoization: the state stays at its position, arguments shift.
    assert_eq!(tree(&composition), "0:t0\n1:t1\n2:t2\n3:t3\n");
    assert_eq!(take_log(), ["Row(0)", "Row(1)", "Row(2)", "Row(3)"]);
    composition.validate();
}

#[test]
fn duplicate_keys_match_in_order() {
    let composition = new_composition();
    let list = mutable_state_of(vec![7u32, 7, 8]);
    let handle = list.handle();
    composition.set_content(move || {
        restartable(key_of("Column"), handle, |list| {
            list.with(|ids| ids.iter().for_each(|&id| key(id, || Row(id))))
        })
    });
    assert_eq!(tree(&composition), "7:t0\n7:t1\n8:t2\n");
    list.set(vec![8, 7]);
    composition.recompose();
    assert_eq!(tree(&composition), "8:t2\n7:t0\n");
    composition.validate();
}

#[test]
fn a_scope_whose_node_count_changes_keeps_sibling_nodes_in_place() {
    let composition = new_composition();
    let count = mutable_state_of(1);
    let handle = count.handle();
    composition.set_content(move || {
        Leaf("first");
        restartable(key_of("Many"), handle, |count| {
            for index in 0..count.get() {
                Leaf(&format!("item{index}"));
            }
        });
        Leaf("last");
    });
    assert_eq!(tree(&composition), "first\nitem0\nlast\n");
    count.set(3);
    composition.recompose();
    assert_eq!(tree(&composition), "first\nitem0\nitem1\nitem2\nlast\n");
    composition.validate();
    count.set(0);
    composition.recompose();
    assert_eq!(tree(&composition), "first\nlast\n");
    composition.validate();
    count.set(2);
    composition.recompose();
    assert_eq!(tree(&composition), "first\nitem0\nitem1\nlast\n");
    composition.validate();
}

#[test]
fn nested_nodes_get_their_children() {
    let composition = new_composition();
    let count = mutable_state_of(1);
    let handle = count.handle();
    composition.set_content(move || {
        let composer = current();
        composer.start_node(key_of("Box"), || Label("box".into()));
        restartable(key_of("Inner"), handle, |count| {
            for index in 0..count.get() {
                Leaf(&format!("child{index}"));
            }
        });
        composer.end_group();
        Leaf("sibling");
    });
    assert_eq!(tree(&composition), "box\n  child0\nsibling\n");
    count.set(2);
    composition.recompose();
    assert_eq!(tree(&composition), "box\n  child0\n  child1\nsibling\n");
    composition.validate();
}

#[test]
fn a_scope_removed_by_its_parent_never_runs() {
    let composition = new_composition();
    let show = mutable_state_of(true);
    let value = mutable_state_of(0);
    let (show_handle, value_handle) = (show.handle(), value.handle());
    composition.set_content(move || {
        restartable(key_of("Parent"), (show_handle, value_handle), |(show, value)| {
            log("Parent");
            if show.get() {
                region(1, || Reader("child", value));
            }
        })
    });
    take_log();
    value.set(1);
    show.set(false);
    composition.recompose();
    assert_eq!(take_log(), ["Parent"]);
    assert_eq!(tree(&composition), "");
    composition.validate();
}

#[test]
fn no_scope_runs_twice_in_a_pass() {
    let composition = new_composition();
    let x = mutable_state_of(0);
    let y = mutable_state_of(0);
    let (x_handle, y_handle) = (x.handle(), y.handle());
    composition.set_content(move || {
        restartable(key_of("Parent"), (x_handle, y_handle), |(x, y)| {
            log("Parent");
            restartable(key_of("Child"), (x.get(), y), |(x, y)| {
                log("Child");
                Leaf(&format!("{x},{}", y.get()));
            });
        })
    });
    take_log();
    x.set(1);
    y.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["Parent", "Child"]);
    assert_eq!(tree(&composition), "1,1\n");
}

#[test]
fn reads_no_longer_made_stop_invalidating() {
    let composition = new_composition();
    let flag = mutable_state_of(true);
    let value = mutable_state_of(0);
    let (flag_handle, value_handle) = (flag.handle(), value.handle());
    composition.set_content(move || {
        restartable(key_of("Reader"), (flag_handle, value_handle), |(flag, value)| {
            log("Reader");
            if flag.get() {
                value.get();
            }
        })
    });
    take_log();
    value.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["Reader"]);
    flag.set(false);
    composition.recompose();
    assert_eq!(take_log(), ["Reader"]);
    value.set(2);
    assert!(!composition.has_invalidations(), "a read from an earlier run is stale");
}

#[test]
fn skipped_scopes_keep_their_reads() {
    let composition = new_composition();
    let parent_state = mutable_state_of(0);
    let child_state = mutable_state_of(0);
    let (p, c) = (parent_state.handle(), child_state.handle());
    composition.set_content(move || {
        restartable(key_of("Parent"), (p, c), |(p, c)| {
            log(format!("Parent {}", p.get()));
            Reader("child", c);
        })
    });
    take_log();
    for round in 1..4 {
        parent_state.set(round);
        composition.recompose();
        assert_eq!(take_log(), [format!("Parent {round}")]);
        child_state.set(round);
        composition.recompose();
        assert_eq!(take_log(), ["child"], "the skipped child still observes its state");
    }
}

#[test]
fn invalidating_the_current_recompose_scope() {
    thread_local! {
        static SCOPE: Cell<Option<RecomposeScope>> = const { Cell::new(None) };
    }
    let composition = new_composition();
    composition.set_content(|| {
        restartable(key_of("Self"), (), |()| {
            log("Self");
            SCOPE.with(|scope| scope.set(Some(current_recompose_scope())));
        })
    });
    take_log();
    let scope = SCOPE.with(|scope| scope.get()).expect("scope");
    assert!(scope.is_valid());
    scope.invalidate();
    assert!(composition.recompose());
    assert_eq!(take_log(), ["Self"]);
    drop(composition);
    assert!(!scope.is_valid());
    scope.invalidate(); // harmless once the scope is gone
}

#[test]
fn compositions_on_one_thread_are_independent() {
    let first = new_composition();
    let second = Composition::new(Tree::new());
    let a = mutable_state_of(0);
    let shared = mutable_state_of(0);
    let (a_handle, shared_handle) = (a.handle(), shared.handle());
    first.set_content(move || {
        Reader("a", a_handle);
        Reader("shared1", shared_handle);
    });
    second.set_content(move || Reader("shared2", shared_handle));
    take_log();
    a.set(1);
    assert!(first.has_invalidations() && !second.has_invalidations());
    shared.set(1);
    assert!(second.has_invalidations());
    first.recompose();
    second.recompose();
    assert_eq!(take_log(), ["a", "shared1", "shared2"]);
    drop(first);
    shared.set(2);
    second.recompose();
    assert_eq!(tree(&second), "shared2=2\n");
}

#[test]
fn a_slot_of_another_type_is_recreated() {
    let composition = new_composition();
    let flag = mutable_state_of(true);
    let handle = flag.handle();
    composition.set_content(move || {
        restartable(key_of("Mixed"), handle, |flag| {
            // No region groups: the two branches share slot 0.
            if flag.get() {
                let token = remember(|| Rc::new(Token::new("int")));
                Leaf(&token.name);
            } else {
                let token = remember(|| (Rc::new(Token::new("string")),));
                Leaf(&token.0.name);
            }
        })
    });
    flag.set(false);
    composition.recompose();
    assert_eq!(tree(&composition), "string\n");
    composition.validate();
    let log = take_log();
    assert_eq!(log, ["create int", "create string", "drop int"]);
}

#[test]
fn remember_keyed_recomputes_when_keys_change() {
    let composition = new_composition();
    let key_state = mutable_state_of(1);
    let handle = key_state.handle();
    composition.set_content(move || {
        restartable(key_of("Keyed"), handle, |key_state| {
            let key = key_state.get();
            let token = remember_keyed(key / 2, || Rc::new(Token::new(format!("k{key}"))));
            Leaf(&token.name);
        })
    });
    key_state.set(0); // same key (0 == 1 / 2)
    composition.recompose();
    assert_eq!(tree(&composition), "k1\n");
    key_state.set(4);
    composition.recompose();
    assert_eq!(tree(&composition), "k4\n");
    assert_eq!(take_log(), ["create k1", "create k4", "drop k1"]);
}

struct Releasing(&'static str);

impl fmt::Debug for Releasing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Drop for Releasing {
    fn drop(&mut self) {
        log(format!("release node {}", self.0));
    }
}

#[test]
fn nodes_are_released_before_the_state_of_their_group() {
    let composition = new_composition();
    let composition = {
        drop(composition);
        Composition::new(NodeTree::<Releasing>::new())
    };
    let show = mutable_state_of(true);
    let handle = show.handle();
    composition.set_content(move || {
        restartable(key_of("Parent"), handle, |show| {
            if show.get() {
                region(1, || {
                    let _state = remember(|| Rc::new(Token::new("state")));
                    let composer = current();
                    composer.start_node(key_of("Node"), || Releasing("outer"));
                    composer.start_node(key_of("Node"), || Releasing("inner"));
                    composer.end_group();
                    composer.end_group();
                });
            }
        })
    });
    take_log();
    show.set(false);
    composition.recompose();
    // Rule R1: the nodes (and the handlers they hold) go before the state their group owned.
    assert_eq!(take_log(), ["release node outer", "release node inner", "drop state"]);
    composition.with_applier(|tree: &mut NodeTree<Releasing>| assert!(tree.is_empty()));
}

#[test]
fn a_retired_cell_lives_until_the_end_of_the_next_pass() {
    let composition = new_composition();
    composition.set_content(|| {});
    let owner = mutable_state_of(5);
    let handle = owner.handle();
    drop(owner);
    // Rule R2: code later in the same handler still sees a live, detached cell.
    assert_eq!(handle.get(), 5);
    handle.set(6);
    assert_eq!(handle.try_get(), Some(6));
    composition.set_content(|| {});
    assert_eq!(handle.try_get(), None);
    assert!(!handle.try_set(1));
    assert!(!handle.is_alive());
}

#[test]
fn a_remembered_owned_state_is_freed_a_pass_after_its_group_leaves() {
    thread_local! {
        static HANDLE: Cell<Option<MutableState<i32>>> = const { Cell::new(None) };
    }
    let composition = new_composition();
    let show = mutable_state_of(true);
    let handle = show.handle();
    composition.set_content(move || {
        restartable(key_of("Parent"), handle, |show| {
            if show.get() {
                region(1, || {
                    let state = remember(|| mutable_state_of(42));
                    HANDLE.with(|handle| handle.set(Some(state)));
                });
            }
        })
    });
    let state = HANDLE.with(|handle| handle.get()).expect("handle");
    show.set(false);
    composition.recompose();
    assert_eq!(state.try_get(), Some(42), "retired at the end of this pass, not freed");
    show.set(true);
    composition.recompose();
    assert_eq!(state.try_get(), None, "freed at the end of the next one");
}

#[test]
fn using_a_freed_handle_panics_naming_where_it_was_created() {
    let owner = mutable_state_of(1); // no composition on this thread: freed at once
    let handle = owner.handle();
    drop(owner);
    let message = panic_message(catch_unwind(|| handle.get()).expect_err("panics"));
    assert!(message.contains("tests/suite/scenarios.rs"), "{message}");
    assert!(message.contains("after it was freed"), "{message}");
    assert_eq!(handle.try_get(), None);
}

#[test]
fn conflicting_access_inside_with_panics_with_both_locations() {
    let state = mutable_state_of(vec![1]);
    let handle = state.handle();
    let message = panic_message(
        catch_unwind(AssertUnwindSafe(|| handle.with(|_| handle.edit(|list| list.push(2)))))
            .expect_err("panics"),
    );
    assert_eq!(message.matches("tests/suite/scenarios.rs").count(), 3, "{message}");
    // Nested reads are fine, and the cell is usable afterwards.
    assert_eq!(handle.with(|outer| handle.with(|inner| outer.len() + inner.len())), 2);
    handle.edit(|list| list.push(3));
    assert_eq!(handle.get(), [1, 3]);
}

#[test]
fn update_and_edit() {
    let state = mutable_state_of(1);
    state.update(|value| value + 1);
    assert_eq!(state.get(), 2);
    let doubled = state.edit(|value| {
        *value *= 2;
        *value
    });
    assert_eq!(doubled, 4);
    let never = mutable_state_of_with(1, never_equal_policy());
    let composition = new_composition();
    let handle = never.handle();
    composition.set_content(move || {
        restartable(key_of("Never"), handle, |state| {
            log("Never");
            state.get();
        })
    });
    take_log();
    never.set(1);
    assert!(composition.has_invalidations(), "never-equal policy invalidates equal writes");
}

/// A write during composition is allowed, as Compose's (D703): a scope writing a state it read
/// recomposes in the next pass, until the value settles.
#[test]
fn a_scope_writing_what_it_read_recomposes_in_the_next_pass() {
    let composition = new_composition();
    let state = mutable_state_of(0);
    let handle = state.handle();
    composition.set_content(move || {
        let value = handle.get();
        log(format!("value {value}"));
        if value < 3 {
            handle.set(value + 1);
        }
    });
    assert_eq!(take_log(), ["value 0"]);
    assert!(composition.recompose());
    assert!(composition.recompose());
    assert!(composition.recompose());
    assert!(!composition.recompose(), "settled");
    assert_eq!(take_log(), ["value 1", "value 2", "value 3"]);
    assert_eq!(state.get(), 3);
}

struct Observer(&'static str);

impl RememberObserver for Observer {
    fn on_remembered(&self) {
        log(format!("remembered {}", self.0));
    }
    fn on_forgotten(&self) {
        log(format!("forgotten {}", self.0));
    }
    fn on_abandoned(&self) {
        log(format!("abandoned {}", self.0));
    }
}

#[test]
fn remember_observers_get_their_callbacks_in_order() {
    let composition = new_composition();
    let show = mutable_state_of(true);
    let handle = show.handle();
    composition.set_content(move || {
        restartable(key_of("Parent"), handle, |show| {
            log("body");
            if show.get() {
                region(1, || {
                    remember_observer(|| Observer("a"));
                    let b = remember_observer(|| Observer("b"));
                    assert_eq!(b.0, "b");
                });
            }
        })
    });
    assert_eq!(take_log(), ["body", "remembered a", "remembered b"]);
    show.set(false);
    composition.recompose();
    assert_eq!(take_log(), ["body", "forgotten b", "forgotten a"]);
}

#[test]
fn side_effects_run_after_the_pass_that_ran_them() {
    let composition = new_composition();
    let value = mutable_state_of(0);
    let other = mutable_state_of(0);
    let (handle, other_handle) = (value.handle(), other.handle());
    composition.set_content(move || {
        restartable(key_of("Effects"), handle, |value| {
            let current = value.get();
            log(format!("body {current}"));
            SideEffect(move || log(format!("effect {current}")));
        });
        Reader("other", other_handle);
    });
    assert_eq!(take_log(), ["body 0", "other", "effect 0"]);
    value.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["body 1", "effect 1"]);
    other.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["other"], "a skipped scope's side effect doesn't run");
}

#[test]
fn side_effects_may_write_state() {
    let composition = new_composition();
    let value = mutable_state_of(0);
    let handle = value.handle();
    composition.set_content(move || {
        restartable(key_of("Writer"), handle, |value| {
            let current = value.get();
            log(format!("body {current}"));
            if current < 2 {
                SideEffect(move || value.set(current + 1));
            }
        })
    });
    assert!(composition.has_invalidations());
    while composition.recompose() {}
    assert_eq!(take_log(), ["body 0", "body 1", "body 2"]);
}

#[test]
fn disposable_effects_clean_up_on_key_change_and_exit() {
    let composition = new_composition();
    let key_state = mutable_state_of(1);
    let show = mutable_state_of(true);
    let (key_handle, show_handle) = (key_state.handle(), show.handle());
    composition.set_content(move || {
        restartable(key_of("Parent"), (key_handle, show_handle), |(key_state, show)| {
            if show.get() {
                region(1, || {
                    let key = key_state.get();
                    DisposableEffect(key, move |scope| {
                        log(format!("effect {key}"));
                        scope.on_dispose(move || log(format!("dispose {key}")))
                    });
                });
            }
        })
    });
    assert_eq!(take_log(), ["effect 1"]);
    key_state.set(2);
    composition.recompose();
    assert_eq!(take_log(), ["dispose 1", "effect 2"]);
    show.set(false);
    composition.recompose();
    assert_eq!(take_log(), ["dispose 2"]);
}

#[test]
fn frame_requests_are_coalesced_until_the_next_pass() {
    let composition = new_composition();
    let requests = Rc::new(Cell::new(0));
    let counter = requests.clone();
    composition.set_frame_requester(move || counter.set(counter.get() + 1));
    let state = mutable_state_of(0);
    let handle = state.handle();
    composition.set_content(move || Reader("r", handle));
    state.set(1);
    state.set(2);
    compose_runtime::snapshots::Snapshot::send_apply_notifications();
    assert_eq!(requests.get(), 1);
    composition.recompose();
    state.set(3);
    compose_runtime::snapshots::Snapshot::send_apply_notifications();
    assert_eq!(requests.get(), 2);
}

#[test]
fn dropping_a_composition_releases_everything() {
    let composition = new_composition();
    let states_before = live_state_count();
    let scopes_before = live_scope_count();
    let show = mutable_state_of(true);
    let handle = show.handle();
    composition.set_content(move || {
        restartable(key_of("Parent"), handle, |show| {
            let _a = remember(|| Rc::new(Token::new("a")));
            if show.get() {
                region(1, || {
                    remember(|| mutable_state_of(String::from("owned")));
                    remember_observer(|| Observer("observer"));
                    Text("text".into());
                });
            }
        })
    });
    take_log();
    drop(composition);
    assert_eq!(take_log(), ["forgotten observer", "drop a"]);
    drop(show);
    assert_eq!(live_state_count(), states_before);
    assert_eq!(live_scope_count(), scopes_before);
}

#[test]
fn a_panic_poisons_the_composition_and_dropping_it_frees_everything_once() {
    let composition = new_composition();
    let states_before = live_state_count();
    let fail = mutable_state_of(false);
    let handle = fail.handle();
    composition.set_content(move || {
        let _outer = remember(|| Rc::new(Token::new("outer")));
        restartable(key_of("Failing"), handle, |fail| {
            let _inner = remember(|| Rc::new(Token::new("inner")));
            remember(|| mutable_state_of(0));
            remember_observer(|| Observer("observer"));
            if fail.get() {
                region(1, || {
                    let _late = remember(|| Rc::new(Token::new("late")));
                    remember_observer(|| Observer("late observer"));
                    Leaf("half-built");
                    panic!("boom");
                });
            }
        });
    });
    take_log();
    fail.set(true);
    let result = catch_unwind(AssertUnwindSafe(|| composition.recompose()));
    assert_eq!(panic_message(result.expect_err("panics")), "boom");
    assert!(composition.is_poisoned());
    let message = panic_message(
        catch_unwind(AssertUnwindSafe(|| composition.recompose())).expect_err("poisoned"),
    );
    assert!(message.contains("poisoned"), "{message}");
    take_log();
    drop(composition);
    let mut log = take_log();
    log.sort();
    assert_eq!(
        log,
        ["abandoned late observer", "drop inner", "drop late", "drop outer", "forgotten observer"]
    );
    drop(fail);
    assert_eq!(live_state_count(), states_before);
}

#[test]
fn protocol_misuse_panics_with_a_clear_message() {
    let message = panic_message(catch_unwind(current).expect_err("outside a pass"));
    assert!(message.contains("outside a composition pass"), "{message}");

    let composition = new_composition();
    let result =
        catch_unwind(AssertUnwindSafe(|| composition.set_content(|| current().end_group())));
    assert!(panic_message(result.expect_err("unbalanced")).contains("unbalanced"));

    let composition = new_composition();
    let result = catch_unwind(AssertUnwindSafe(|| {
        composition.set_content(|| current().start_replace_group(1))
    }));
    // The content's restart group ends while the replace group is still open.
    assert!(panic_message(result.expect_err("unclosed")).contains("unbalanced"));

    // A composer works in any pass on its thread: a lambda may capture it for later passes.
    thread_local! {
        static KEPT: Cell<Option<Composer>> = const { Cell::new(None) };
    }
    let composition = new_composition();
    composition.set_content(|| KEPT.with(|kept| kept.set(Some(current()))));
    let kept = KEPT.with(|kept| kept.get()).expect("composer");
    composition.set_content(move || {
        kept.start_replace_group(1);
        Leaf("later");
        kept.end_group();
    });
    assert_eq!(tree(&composition), "later\n");
    let message = panic_message(catch_unwind(|| kept.start_replace_group(1)).expect_err("no pass"));
    assert!(message.contains("outside a composition pass"), "{message}");

    let composition = new_composition();
    let result = catch_unwind(AssertUnwindSafe(|| {
        composition.set_content(|| {
            restartable(key_of("Nested"), (), |()| {
                remember(|| remember(|| 1) + 1);
            })
        })
    }));
    assert!(panic_message(result.expect_err("nested remember")).contains("out of order"));
}

#[test]
fn set_content_again_recomposes_against_the_old_content() {
    let composition = new_composition();
    composition.set_content(|| Row(1));
    assert_eq!(tree(&composition), "1:t0\n");
    composition.set_content(|| {
        Row(2);
        Row(3);
    });
    // Same keys at the same position: the first row keeps its state.
    assert_eq!(tree(&composition), "2:t0\n3:t1\n");
    composition.validate();
}
