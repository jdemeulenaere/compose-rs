//! Modifier locals (docs/parity/design/modifier-nodes.md §3.9, step MN-9).

use crate::support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::UiHost;
use compose_ui::*;
use support::*;

const DEFAULT: &str = "Default Value";

static LOCAL_STRING: ModifierLocal<&'static str> = modifier_local_of("string", || DEFAULT);
static LOCAL_STRING_2: ModifierLocal<&'static str> = modifier_local_of("string 2", || DEFAULT);
static LOCAL_INTEGER: ModifierLocal<i32> = modifier_local_of("integer", || i32::MIN);
static LOCAL_UNSET: ModifierLocal<&'static str> =
    modifier_local_of("unset", || panic!("No default value"));

type Read = Rc<Cell<&'static str>>;

fn read() -> Read {
    Rc::new(Cell::new(""))
}

/// `Modifier.modifierLocalConsumer { read = LOCAL_STRING.current }`. The tests make it outside
/// their content and clone it in: the same element, so its node runs because the manager found
/// it (a provider's change), not because its element is new.
fn consumer(read: &Read) -> Modifier {
    Modifier.modifier_local_consumer({
        let read = read.clone();
        move |scope| read.set(scope.current(&LOCAL_STRING))
    })
}

fn provider(value: &'static str) -> Modifier {
    Modifier.modifier_local_provider(&LOCAL_STRING, move || value)
}

// ---- ModifierLocalMapTest ----------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMapTest.kt, empty modifier local map
#[test]
fn an_empty_map_has_nothing() {
    let map = modifier_local_map_of();
    assert!(!map.contains(&LOCAL_STRING));
    assert_eq!(map.get(&LOCAL_STRING), None);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMapTest.kt, modifier local map with a single null entry
#[test]
fn a_map_of_a_key_has_no_value_for_it() {
    let map = modifier_local_map_of().with_key(&LOCAL_STRING);
    assert!(map.contains(&LOCAL_STRING));
    assert_eq!(map.get(&LOCAL_STRING), None);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMapTest.kt, modifier local map with two null entries
#[test]
fn a_map_of_two_keys_has_no_values() {
    let map = modifier_local_map_of().with_key(&LOCAL_STRING).with_key(&LOCAL_INTEGER);
    assert_eq!(map.get(&LOCAL_STRING), None);
    assert_eq!(map.get(&LOCAL_INTEGER), None);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMapTest.kt, modifier local map with three null entries
#[test]
fn a_map_of_three_keys_has_no_values() {
    let map = modifier_local_map_of()
        .with_key(&LOCAL_STRING)
        .with_key(&LOCAL_STRING_2)
        .with_key(&LOCAL_INTEGER);
    assert_eq!(map.get(&LOCAL_STRING), None);
    assert_eq!(map.get(&LOCAL_STRING_2), None);
    assert_eq!(map.get(&LOCAL_INTEGER), None);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMapTest.kt, modifier local map with a single entry
#[test]
fn a_map_of_a_value_has_it() {
    let map = modifier_local_map_of().with(&LOCAL_STRING, "single");
    assert_eq!(map.get(&LOCAL_STRING), Some("single"));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMapTest.kt, modifier local map with two values
#[test]
fn a_map_of_two_values_has_them() {
    let map = modifier_local_map_of().with(&LOCAL_STRING, "first").with(&LOCAL_INTEGER, 2);
    assert_eq!(map.get(&LOCAL_STRING), Some("first"));
    assert_eq!(map.get(&LOCAL_INTEGER), Some(2));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMapTest.kt, modifier local map with three values
#[test]
fn a_map_of_three_values_has_them() {
    let map = modifier_local_map_of()
        .with(&LOCAL_STRING, "first")
        .with(&LOCAL_STRING_2, "second")
        .with(&LOCAL_INTEGER, 3);
    assert_eq!(map.get(&LOCAL_STRING), Some("first"));
    assert_eq!(map.get(&LOCAL_STRING_2), Some("second"));
    assert_eq!(map.get(&LOCAL_INTEGER), Some(3));
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/modifier/ModifierLocal.kt, modifierLocalOf
#[test]
fn two_locals_of_one_name_are_two() {
    static FIRST: ModifierLocal<i32> = modifier_local_of("one name", || 0);
    static SECOND: ModifierLocal<i32> = modifier_local_of("one name", || 0);
    let map = modifier_local_map_of().with(&FIRST, 1);
    assert_eq!(map.get(&SECOND), None);
    assert_eq!(FIRST.name(), SECOND.name());
}

// ---- ModifierLocalSameLayoutNodeTest -----------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, exceptionInsideDefaultFactory
#[test]
#[should_panic(expected = "No default value")]
fn a_panic_in_a_default_reaches_the_reader() {
    let _host = host(|| {
        Leaf(Modifier.modifier_local_consumer(|scope| {
            scope.current(&LOCAL_UNSET);
        }));
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, defaultValue
#[test]
fn a_local_nothing_provides_reads_its_default() {
    let value = read();
    let v = consumer(&value);
    let _host = host(move || Leaf(v.clone()));
    assert_eq!(value.get(), DEFAULT);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, doesNotReadValuesProvidedAfterThisModifier
#[test]
fn what_s_provided_after_a_consumer_isn_t_read() {
    let value = read();
    let v = consumer(&value);
    let _host = host(move || Leaf(v.clone().then(provider("Provided"))));
    assert_eq!(value.get(), DEFAULT);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, readValueProvidedImmediatelyBeforeThisModifier
#[test]
fn what_s_provided_just_before_a_consumer_is_read() {
    let value = read();
    let v = consumer(&value);
    let _host = host(move || Leaf(provider("Provided").then(v.clone())));
    assert_eq!(value.get(), "Provided");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, readValueProvidedBeforeThisModifier
#[test]
fn what_s_provided_before_a_consumer_is_read_past_other_modifiers() {
    let value = read();
    let v = consumer(&value);
    let _host = host(move || Leaf(provider("Provided").size(100.dp()).then(v.clone())));
    assert_eq!(value.get(), "Provided");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, readsTheLastValueProvidedBeforeThisModifier
#[test]
fn the_last_provided_before_a_consumer_is_read() {
    let value = read();
    let v = consumer(&value);
    let _host = host(move || Leaf(provider("1").then(provider("2")).then(v.clone())));
    assert_eq!(value.get(), "2");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, multipleModifierLocalsOfSameDataType
#[test]
fn two_locals_of_one_type_are_each_read() {
    let seen = Rc::new(RefCell::new((DEFAULT, DEFAULT)));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        Leaf(
            Modifier
                .modifier_local_provider(&LOCAL_STRING, || "1")
                .modifier_local_provider(&LOCAL_STRING_2, || "2")
                .modifier_local_consumer(move |scope| {
                    *s.borrow_mut() =
                        (scope.current(&LOCAL_STRING), scope.current(&LOCAL_STRING_2));
                }),
        );
    });
    assert_eq!(*seen.borrow(), ("1", "2"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, multipleModifierLocalsWithDifferentDataType
#[test]
fn two_locals_of_different_types_are_each_read() {
    let seen = Rc::new(RefCell::new((DEFAULT, 0)));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        Leaf(
            Modifier
                .modifier_local_provider(&LOCAL_STRING, || "Provided")
                .modifier_local_provider(&LOCAL_INTEGER, || 100)
                .modifier_local_consumer(move |scope| {
                    *s.borrow_mut() = (scope.current(&LOCAL_STRING), scope.current(&LOCAL_INTEGER));
                }),
        );
    });
    assert_eq!(*seen.borrow(), ("Provided", 100));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, modifierLocalProviderChanged
#[test]
fn a_new_provider_gives_its_value() {
    let value = read();
    let first = mutable_state_of(true);
    let (v, f) = (consumer(&value), first.handle());
    let mut host = host(move || {
        Leaf(provider(if f.get() { "Provider1" } else { "Provider2" }).then(v.clone()));
    });
    assert_eq!(value.get(), "Provider1");
    first.set(false);
    host.frame();
    assert_eq!(value.get(), "Provider2");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, modifierLocalProviderChanged_returnsDefaultValueBeforeNewValue
#[test]
fn a_new_provider_gives_its_value_once_with_nothing_between() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let first = mutable_state_of(true);
    let (v, f) = (values.clone(), first.handle());
    let mut host = host(move || {
        let v = v.clone();
        Leaf(provider(if f.get() { "Provider1" } else { "Provider2" }).modifier_local_consumer(
            move |scope| v.borrow_mut().push(scope.current(&LOCAL_STRING)),
        ));
    });
    first.set(false);
    host.frame();
    assert_eq!(&*values.borrow(), &["Provider1", "Provider2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, modifierLocalConsumer_returnsDefaultValueWhenModifierIsDisposed
#[test]
fn a_provider_gone_gives_the_default() {
    let value = read();
    let has = mutable_state_of(true);
    let (v, h) = (consumer(&value), has.handle());
    let mut host = host(move || {
        let modifier = if h.get() { provider("Provided") } else { Modifier };
        Leaf(modifier.then(v.clone()));
    });
    assert_eq!(value.get(), "Provided");
    has.set(false);
    host.frame();
    assert_eq!(value.get(), DEFAULT);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, modifierLocalConsumer_returnsDefaultValueWhenComposableIsDisposed
#[test]
fn a_consumer_leaving_reads_the_default() {
    let value = read();
    let include = mutable_state_of(true);
    let (v, i) = (consumer(&value), include.handle());
    let mut host = host(move || {
        if i.get() {
            Leaf(provider("Provided").then(v.clone()));
        }
    });
    assert_eq!(value.get(), "Provided");
    include.set(false);
    host.frame();
    assert_eq!(value.get(), DEFAULT);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, modifierLocalProviderValueChanged
#[test]
fn a_provider_s_new_value_is_read() {
    let value = read();
    let provided = mutable_state_of("Value 1");
    let (v, p) = (consumer(&value), provided.handle());
    let mut host = host(move || {
        Leaf(Modifier.modifier_local_provider(&LOCAL_STRING, move || p.get()).then(v.clone()));
    });
    assert_eq!(value.get(), "Value 1");
    provided.set("Value 2");
    host.frame();
    assert_eq!(value.get(), "Value 2");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, modifierLocalProviderAdded
#[test]
fn a_provider_added_is_read() {
    let value = read();
    let has = mutable_state_of(false);
    let (v, h) = (consumer(&value), has.handle());
    let mut host = host(move || {
        let modifier = if h.get() { provider("Provided") } else { Modifier };
        Leaf(modifier.then(v.clone()));
    });
    assert_eq!(value.get(), DEFAULT);
    has.set(true);
    host.frame();
    assert_eq!(value.get(), "Provided");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, modifierLocalProviderRemoved_readsPreviousParent
#[test]
fn a_provider_removed_gives_the_one_before_it() {
    let value = read();
    let has = mutable_state_of(true);
    let (v, h) = (consumer(&value), has.handle());
    let mut host = host(move || {
        let second = if h.get() { provider("Parent2") } else { Modifier };
        Leaf(provider("Parent1").then(second).then(v.clone()));
    });
    assert_eq!(value.get(), "Parent2");
    has.set(false);
    host.frame();
    assert_eq!(value.get(), "Parent1");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, modifierLocalProviderMoved_readsDefaultValue
#[test]
fn a_provider_moved_after_a_consumer_isn_t_read() {
    let value = read();
    let moved = mutable_state_of(false);
    let (v, m) = (consumer(&value), moved.handle());
    let mut host = host(move || {
        let before = if m.get() { Modifier } else { provider("Provided") };
        let after = if m.get() { provider("Provided") } else { Modifier };
        Leaf(before.then(v.clone()).then(after));
    });
    assert_eq!(value.get(), "Provided");
    moved.set(true);
    host.frame();
    assert_eq!(value.get(), DEFAULT);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, modifierLocalProviderMoved_readsPreviousParent
#[test]
fn a_provider_moved_after_a_consumer_gives_the_one_before_it() {
    let value = read();
    let moved = mutable_state_of(false);
    let (v, m) = (consumer(&value), moved.handle());
    let mut host = host(move || {
        let before = if m.get() { Modifier } else { provider("Parent2") };
        let after = if m.get() { provider("Parent2") } else { Modifier };
        Leaf(provider("Parent1").then(before).then(v.clone()).then(after));
    });
    assert_eq!(value.get(), "Parent2");
    moved.set(true);
    host.frame();
    assert_eq!(value.get(), "Parent1");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, modifierLocalProviderMoved_readsSameValue
#[test]
fn a_provider_moved_still_before_a_consumer_gives_the_same() {
    let value = read();
    let moved = mutable_state_of(false);
    let (v, m) = (consumer(&value), moved.handle());
    let mut host = host(move || {
        let before = if m.get() { Modifier } else { provider("Provided") };
        let after = if m.get() { provider("Provided") } else { Modifier };
        Leaf(before.size(100.dp()).then(after).then(v.clone()));
    });
    moved.set(true);
    host.frame();
    assert_eq!(value.get(), "Provided");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalSameLayoutNodeTest.kt, modifierLocalCallsOnce
#[test]
fn one_change_of_a_provider_runs_a_consumer_once() {
    let calls = Rc::new(Cell::new(0));
    let first = mutable_state_of(true);
    let c = calls.clone();
    let consumer = Modifier.modifier_local_consumer(move |scope| {
        scope.current(&LOCAL_STRING);
        c.set(c.get() + 1);
    });
    let provider_1 = provider("Provided");
    let provider_2 = provider("Another");
    let f = first.handle();
    let mut host = host(move || {
        let chosen = if f.get() { provider_1.clone() } else { provider_2.clone() };
        Leaf(chosen.then(consumer.clone()));
    });
    calls.set(0);
    first.set(false);
    host.frame();
    assert_eq!(calls.get(), 1);
}

// ---- ModifierLocalMultiLayoutNodeTest ----------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, doesNotReadValuesProvidedByChildren
#[test]
fn what_a_child_provides_isn_t_read_above_it() {
    let value = read();
    let v = consumer(&value);
    let _host = host(move || {
        Stack!(modifier = v.clone(), {
            Leaf(provider("Provided"));
        });
    });
    assert_eq!(value.get(), DEFAULT);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, readsValuesProvidedByParent
#[test]
fn what_a_parent_provides_is_read() {
    let value = read();
    let v = consumer(&value);
    let _host = host(move || {
        let v = v.clone();
        Stack!(modifier = provider("Provided"), {
            Leaf(v.clone());
        });
    });
    assert_eq!(value.get(), "Provided");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, readsValuesProvidedByGrandParent
#[test]
fn what_a_grandparent_provides_is_read() {
    let value = read();
    let v = consumer(&value);
    let _host = host(move || {
        let v = v.clone();
        Stack!(modifier = provider("Provided"), {
            let v = v.clone();
            Stack!({
                Leaf(v.clone());
            });
        });
    });
    assert_eq!(value.get(), "Provided");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, parentInSameLayoutNodeTakesPrecedenceOverParentLayoutNode
#[test]
fn a_provider_in_the_same_layout_node_wins_over_a_parent_s() {
    let value = read();
    let v = consumer(&value);
    let _host = host(move || {
        let v = v.clone();
        Stack!(modifier = provider("Parent"), {
            Leaf(provider("Same").then(v.clone()));
        });
    });
    assert_eq!(value.get(), "Same");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, readsValuesProvidedByNearestParent
#[test]
fn the_nearest_parent_s_value_is_read() {
    let value = read();
    let v = consumer(&value);
    let _host = host(move || {
        let v = v.clone();
        Stack!(modifier = provider("Grandparent"), {
            let v = v.clone();
            Stack!(modifier = provider("Parent"), {
                Leaf(v.clone());
            });
        });
    });
    assert_eq!(value.get(), "Parent");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, multipleModifierLocalsOfSameDataType
#[test]
fn two_locals_of_one_type_from_a_parent_are_each_read() {
    let seen = Rc::new(RefCell::new((DEFAULT, DEFAULT)));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        let parent = Modifier
            .modifier_local_provider(&LOCAL_STRING, || "1")
            .modifier_local_provider(&LOCAL_STRING_2, || "2");
        Stack!(modifier = parent, {
            let s = s.clone();
            Leaf(Modifier.modifier_local_consumer(move |scope| {
                *s.borrow_mut() = (scope.current(&LOCAL_STRING), scope.current(&LOCAL_STRING_2));
            }));
        });
    });
    assert_eq!(*seen.borrow(), ("1", "2"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, multipleModifierLocalsWithDifferentDataType
#[test]
fn two_locals_of_different_types_from_a_parent_are_each_read() {
    let seen = Rc::new(RefCell::new((DEFAULT, 0)));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        let parent = Modifier
            .modifier_local_provider(&LOCAL_STRING, || "Provided")
            .modifier_local_provider(&LOCAL_INTEGER, || 100);
        Stack!(modifier = parent, {
            let s = s.clone();
            Leaf(Modifier.modifier_local_consumer(move |scope| {
                *s.borrow_mut() = (scope.current(&LOCAL_STRING), scope.current(&LOCAL_INTEGER));
            }));
        });
    });
    assert_eq!(*seen.borrow(), ("Provided", 100));
}

/// A consumer below `parent`, in a composable that skips (its parent changes alone).
fn below(parent: impl Fn() -> Modifier + 'static) -> (UiHost, Read) {
    let value = read();
    let v = consumer(&value);
    let host = host(move || {
        let v = v.clone();
        Stack!(modifier = parent(), {
            Leaf(v.clone());
        });
    });
    (host, value)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, modifierLocalProviderChanged
#[test]
fn a_parent_s_new_provider_gives_its_value() {
    let first = mutable_state_of(true);
    let f = first.handle();
    let (mut host, value) =
        below(move || provider(if f.get() { "Provider1" } else { "Provider2" }));
    assert_eq!(value.get(), "Provider1");
    first.set(false);
    host.frame();
    assert_eq!(value.get(), "Provider2");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, modifierLocalProviderValueChanged
#[test]
fn a_parent_provider_s_new_value_is_read() {
    let provided = mutable_state_of("Value 1");
    let p = provided.handle();
    let (mut host, value) =
        below(move || Modifier.modifier_local_provider(&LOCAL_STRING, move || p.get()));
    assert_eq!(value.get(), "Value 1");
    provided.set("Value 2");
    host.frame();
    assert_eq!(value.get(), "Value 2");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, modifierLocalProviderAdded
#[test]
fn a_parent_s_provider_added_is_read() {
    let has = mutable_state_of(false);
    let h = has.handle();
    let (mut host, value) = below(move || if h.get() { provider("Provided") } else { Modifier });
    assert_eq!(value.get(), DEFAULT);
    has.set(true);
    host.frame();
    assert_eq!(value.get(), "Provided");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, modifierLocalProviderRemoved_readsDefaultValue
#[test]
fn a_parent_s_provider_removed_gives_the_default() {
    let has = mutable_state_of(true);
    let h = has.handle();
    let (mut host, value) = below(move || if h.get() { provider("Provided") } else { Modifier });
    assert_eq!(value.get(), "Provided");
    has.set(false);
    host.frame();
    assert_eq!(value.get(), DEFAULT);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, modifierLocalProviderRemoved_readsPreviousParent
#[test]
fn a_parent_s_provider_removed_gives_the_one_above() {
    let has = mutable_state_of(true);
    let value = read();
    let (v, h) = (consumer(&value), has.handle());
    let mut host = host(move || {
        let v = v.clone();
        Stack!(modifier = provider("Grandparent"), {
            let v = v.clone();
            let parent = if h.get() { provider("Parent") } else { Modifier };
            Stack!(modifier = parent, {
                Leaf(v.clone());
            });
        });
    });
    assert_eq!(value.get(), "Parent");
    has.set(false);
    host.frame();
    assert_eq!(value.get(), "Grandparent");
}

fn moved_provider(
    grandparent: Option<&'static str>,
    grandchild_reads: &'static str,
    expected: &'static str,
) {
    let moved = mutable_state_of(false);
    let value = read();
    let (v, m) = (consumer(&value), moved.handle());
    let mut host = host(move || {
        let v = v.clone();
        let top = grandparent.map_or(Modifier, provider);
        Stack!(modifier = top, {
            let v = v.clone();
            let parent = if m.get() { Modifier } else { provider("Moved") };
            Stack!(modifier = parent, {
                let child = if m.get() { provider("Moved") } else { Modifier };
                Leaf(v.clone().then(child));
            });
        });
    });
    assert_eq!(value.get(), grandchild_reads);
    moved.set(true);
    host.frame();
    assert_eq!(value.get(), expected);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, modifierLocalProviderMoved_readsDefaultValue
#[test]
fn a_provider_moved_below_a_consumer_gives_the_default() {
    moved_provider(None, "Moved", DEFAULT);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, modifierLocalProviderMoved_readsPreviousParent
#[test]
fn a_provider_moved_below_a_consumer_gives_the_one_above() {
    moved_provider(Some("Grandparent"), "Moved", "Grandparent");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalMultiLayoutNodeTest.kt, modifierLocalProviderMoved_readsSameValue
#[test]
fn a_provider_moved_still_above_a_consumer_gives_the_same() {
    let moved = mutable_state_of(false);
    let value = read();
    let (v, m) = (consumer(&value), moved.handle());
    let mut host = host(move || {
        let v = v.clone();
        let top = if m.get() { Modifier } else { provider("Moved") };
        Stack!(modifier = top, {
            let v = v.clone();
            let parent = if m.get() { provider("Moved") } else { Modifier };
            Stack!(modifier = parent, {
                Leaf(v.clone());
            });
        });
    });
    moved.set(true);
    host.frame();
    assert_eq!(value.get(), "Moved");
}

// ---- ModifierLocalProviderConsumerOrderTest ----------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalProviderConsumerOrderTest.kt, modifierDoesNotConsumeTheValueItProduced
#[test]
fn a_node_doesn_t_read_what_it_provides() {
    let value = read();
    let v = consumer(&value);
    let _host = host(move || Leaf(v.clone().then(provider("Provided"))));
    assert_eq!(value.get(), DEFAULT);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalProviderConsumerOrderTest.kt, modifierConsumesTheValueProducedByParent
#[test]
fn a_node_reads_what_its_parent_provides_not_what_it_provides() {
    let value = read();
    let v = consumer(&value);
    let _host = host(move || {
        let v = v.clone();
        Stack!(modifier = provider("Parent"), {
            Leaf(v.clone().then(provider("Provided")));
        });
    });
    assert_eq!(value.get(), "Parent");
}

fn three_levels(outer: Option<&'static str>, show_inner: bool) -> (UiHost, Read, OwnedState<bool>) {
    let inner = mutable_state_of(show_inner);
    let value = read();
    let (v, i) = (consumer(&value), inner.handle());
    let host = host(move || {
        let v = v.clone();
        Stack!(modifier = outer.map_or(Modifier, provider), {
            let v = v.clone();
            let modifier = if i.get() { provider("Inner") } else { Modifier };
            Stack!(modifier = modifier, {
                Leaf(v.clone().then(provider("")));
            });
        });
    });
    (host, value, inner)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalProviderConsumerOrderTest.kt, detachedProducerStopsProducing
#[test]
fn a_provider_leaving_gives_the_one_above_to_a_grandchild() {
    let (mut host, value, inner) = three_levels(Some("Outer"), true);
    assert_eq!(value.get(), "Inner");
    inner.set(false);
    host.frame();
    assert_eq!(value.get(), "Outer");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalProviderConsumerOrderTest.kt, detachedProducerChangesToDefault
#[test]
fn a_provider_leaving_gives_the_default_to_a_grandchild() {
    let (mut host, value, inner) = three_levels(None, true);
    assert_eq!(value.get(), "Inner");
    inner.set(false);
    host.frame();
    assert_eq!(value.get(), DEFAULT);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalProviderConsumerOrderTest.kt, attachedProducerStartsProducing
#[test]
fn a_provider_coming_is_read_by_a_grandchild_instead_of_the_one_above() {
    let (mut host, value, inner) = three_levels(Some("Outer"), false);
    assert_eq!(value.get(), "Outer");
    inner.set(true);
    host.frame();
    assert_eq!(value.get(), "Inner");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalProviderConsumerOrderTest.kt, attachedProducerStartsProducingFromDefault
#[test]
fn a_provider_coming_is_read_by_a_grandchild_instead_of_the_default() {
    let (mut host, value, inner) = three_levels(None, false);
    assert_eq!(value.get(), DEFAULT);
    inner.set(true);
    host.frame();
    assert_eq!(value.get(), "Inner");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierLocalProviderConsumerOrderTest.kt, changeModifierLocals
#[test]
fn a_new_value_in_a_recomposition_reaches_a_child_that_skips() {
    let provided = mutable_state_of("Value 1");
    let value = read();
    let (v, p) = (consumer(&value), provided.handle());
    let mut host = host(move || {
        let v = v.clone();
        let current = p.get();
        Stack!(modifier = provider(current), {
            Leaf(v.clone().then(provider("")));
        });
    });
    assert_eq!(value.get(), "Value 1");
    provided.set("Value 2");
    host.frame();
    assert_eq!(value.get(), "Value 2");
}

// ---- ModifierLocalConsumerEntityTest -----------------------------------------------------------

/// A host showing `modifier` while `shown` is, kept for reuse otherwise (Compose's test attaches
/// and detaches its node).
fn entity(modifier: impl Fn() -> Modifier + 'static, attached: bool) -> (UiHost, OwnedState<bool>) {
    let shown = mutable_state_of(attached);
    let s = shown.handle();
    let modifier = Rc::new(modifier);
    let host = host(move || {
        let modifier = modifier.clone();
        ReusableContentHost(s.get(), move || {
            let modifier = modifier.clone();
            ReusableContent(0, move || Leaf(modifier()));
        });
    });
    (host, shown)
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/ModifierLocalConsumerEntityTest.kt, unattached modifier local consumer does not invoke lambda
#[test]
fn a_consumer_not_attached_isn_t_run() {
    let value = read();
    let v = consumer(&value);
    let (_host, _) = entity(move || v.clone(), false);
    assert_eq!(value.get(), "");
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/ModifierLocalConsumerEntityTest.kt, changing the consumer modifier with no provider reads default value
#[test]
fn a_new_consumer_with_no_provider_reads_the_default() {
    let value = read();
    let version = mutable_state_of(0);
    let (v, n) = (value.clone(), version.handle());
    let mut host = host(move || {
        n.get();
        Leaf(consumer(&v));
    });
    value.set("");
    version.set(1);
    host.frame();
    assert_eq!(value.get(), DEFAULT);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/ModifierLocalConsumerEntityTest.kt, detached modifier local consumer with no provider invokes with default providers
#[test]
fn a_consumer_detached_with_no_provider_reads_the_default() {
    let value = read();
    let v = consumer(&value);
    let (mut host, shown) = entity(move || v.clone(), true);
    value.set("");
    shown.set(false);
    host.frame();
    assert_eq!(value.get(), DEFAULT);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/ModifierLocalConsumerEntityTest.kt, unattached modifier local consumer with provider does not invoke lambda
#[test]
fn a_consumer_not_attached_with_a_provider_isn_t_run() {
    let value = read();
    let v = consumer(&value);
    let (_host, _) = entity(move || provider("Provided").then(v.clone()), false);
    assert_eq!(value.get(), "");
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/ModifierLocalConsumerEntityTest.kt, changing provided value causes consumer to receive new provided value
#[test]
fn a_provided_value_changing_runs_the_consumer_with_it() {
    let provided = mutable_state_of("Initial");
    let value = read();
    let (v, p) = (consumer(&value), provided.handle());
    let (mut host, _) = entity(
        move || Modifier.modifier_local_provider(&LOCAL_STRING, move || p.get()).then(v.clone()),
        true,
    );
    assert_eq!(value.get(), "Initial");
    provided.set("Final");
    host.frame();
    assert_eq!(value.get(), "Final");
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/ModifierLocalConsumerEntityTest.kt, changing provided value after detaching modifier does not invoke consumer lambda
#[test]
fn a_provided_value_changing_after_a_detach_doesn_t_run_the_consumer() {
    let provided = mutable_state_of("Initial");
    let value = read();
    let (v, p) = (consumer(&value), provided.handle());
    let (mut host, shown) = entity(
        move || Modifier.modifier_local_provider(&LOCAL_STRING, move || p.get()).then(v.clone()),
        true,
    );
    shown.set(false);
    host.frame();
    value.set("");
    provided.set("Final");
    host.frame();
    assert_eq!(value.get(), "");
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/ModifierLocalConsumerEntityTest.kt, changing the consumer modifier with provider reads provided value
#[test]
fn a_new_consumer_with_a_provider_reads_its_value() {
    let value = read();
    let version = mutable_state_of(0);
    let (v, n) = (consumer(&value), version.handle());
    let mut host = host(move || {
        n.get();
        Leaf(provider("Provided").then(v.clone()));
    });
    value.set("");
    version.set(1);
    host.frame();
    assert_eq!(value.get(), "Provided");
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/ModifierLocalConsumerEntityTest.kt, detached modifier local consumer with provider invokes with default provider
#[test]
fn a_consumer_detached_with_a_provider_reads_the_default() {
    let value = read();
    let v = consumer(&value);
    let (mut host, shown) = entity(move || provider("Provided").then(v.clone()), true);
    value.set("");
    shown.set(false);
    host.frame();
    assert_eq!(value.get(), DEFAULT);
}

// ---- ModifierLocalModifierNode -----------------------------------------------------------------

/// Provides `LOCAL_STRING`, from a map it may change, as a `ModifierLocalModifierNode`.
struct Providing(Rc<RefCell<Option<NodeHandle>>>);
struct ProvidingNode(ModifierLocalMap, Rc<RefCell<Option<NodeHandle>>>);

impl ModifierNodeElement for Providing {
    type Node = ProvidingNode;
    fn create(&self) -> ProvidingNode {
        ProvidingNode(modifier_local_map_of().with(&LOCAL_STRING, "First"), self.0.clone())
    }
    fn update(&self, _: &mut ProvidingNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for ProvidingNode {
    fn as_modifier_local(&mut self) -> Option<&mut dyn ModifierLocalModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        *self.1.borrow_mut() = Some(context.handle());
    }
}

impl ModifierLocalModifierNode for ProvidingNode {
    fn provided_values(&mut self) -> Option<&mut ModifierLocalMap> {
        Some(&mut self.0)
    }
}

/// Reads with `modifier_local_current`.
struct Reading(Rc<RefCell<Option<NodeHandle>>>);
struct ReadingNode(Rc<RefCell<Option<NodeHandle>>>);

impl ModifierNodeElement for Reading {
    type Node = ReadingNode;
    fn create(&self) -> ReadingNode {
        ReadingNode(self.0.clone())
    }
    fn update(&self, _: &mut ReadingNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for ReadingNode {
    fn as_modifier_local(&mut self) -> Option<&mut dyn ModifierLocalModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        *self.0.borrow_mut() = Some(context.handle());
    }
}

impl ModifierLocalModifierNode for ReadingNode {}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/modifier/ModifierLocalModifierNode.kt, provide
#[test]
fn a_node_provides_from_its_map_and_a_new_value_is_read_but_not_told() {
    let provider_handle = Rc::new(RefCell::new(None));
    let reader_handle = Rc::new(RefCell::new(None));
    let (p, r) = (provider_handle.clone(), reader_handle.clone());
    let _host = host(move || {
        let r = r.clone();
        Stack!(modifier = Modifier.then_custom(Providing(p.clone())), {
            Leaf(Modifier.then_custom(Reading(r.clone())));
        });
    });
    let reader: NodeHandle = reader_handle.borrow().unwrap();
    let current = || {
        reader
            .with_node(|_: &mut ReadingNode, context| context.modifier_local_current(&LOCAL_STRING))
            .unwrap()
    };
    assert_eq!(current(), "First");
    let provider: NodeHandle = provider_handle.borrow().unwrap();
    provider
        .with_node(|node: &mut ProvidingNode, _| node.0.provide(&LOCAL_STRING, "Second"))
        .unwrap();
    assert_eq!(current(), "Second");
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/modifier/ModifierLocalModifierNode.kt, provide
#[test]
#[should_panic(expected = "must be initially provided")]
fn providing_a_key_not_in_the_map_panics() {
    modifier_local_map_of().with(&LOCAL_STRING, "First").provide(&LOCAL_STRING_2, "Second");
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/modifier/ModifierLocalModifierNode.kt, current
#[test]
#[should_panic(expected = "from an unattached node")]
fn reading_detached_panics() {
    let active = mutable_state_of(true);
    let held = Rc::new(RefCell::new(None));
    let (a, h) = (active.handle(), held.clone());
    let mut host = host(move || {
        let h = h.clone();
        ReusableContentHost(a.get(), move || {
            let h = h.clone();
            ReusableContent(0, move || Leaf(Modifier.then_custom(Reading(h.clone()))));
        });
    });
    active.set(false);
    host.frame();
    let node: NodeHandle = held.borrow().unwrap();
    node.with_node(|_: &mut ReadingNode, context| context.modifier_local_current(&LOCAL_STRING));
}

/// Below a provider of the same local, a change above it isn't seen: nothing to run for.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/modifier/ModifierLocalManager.kt, invalidateConsumersOfNodeForKey
#[test]
fn a_change_above_another_provider_of_it_runs_nothing_below_that() {
    let runs = Rc::new(Cell::new(0));
    let top = mutable_state_of("Top");
    let r = runs.clone();
    let reader = Modifier.modifier_local_consumer(move |scope| {
        scope.current(&LOCAL_STRING);
        r.set(r.get() + 1);
    });
    let t = top.handle();
    let middle = provider("Middle");
    let mut host = host(move || {
        let (reader, middle) = (reader.clone(), middle.clone());
        Stack!(modifier = provider(t.get()), {
            let reader = reader.clone();
            Stack!(modifier = middle.clone(), {
                Leaf(reader.clone());
            });
        });
    });
    assert_eq!(runs.get(), 1);
    top.set("New");
    host.frame();
    assert_eq!(runs.get(), 1);
}
