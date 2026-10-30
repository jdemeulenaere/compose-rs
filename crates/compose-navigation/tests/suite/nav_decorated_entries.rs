//! `remember_decorated_nav_entries`, against Navigation 3's `DecoratedNavEntriesTest`: what
//! decorators wrap and in what order, when they're told of a pop, and what happens with several
//! back stacks. Keys are `i32`s and strings where Kotlin's are `Any`.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use compose_navigation::*;
use compose_runtime::snapshots::{OwnedStateList, mutable_state_list_from};
use compose_runtime::*;
use compose_test::*;

type Log = Rc<RefCell<Vec<String>>>;

fn log_of(log: &Log) -> Vec<String> {
    log.borrow().clone()
}

/// A decorator logging its pops as `name`, and showing the entry.
fn pop_logging(name: &'static str, log: &Log) -> NavEntryDecorator<i32> {
    let log = log.clone();
    NavEntryDecorator::with_on_pop(
        move |_| log.borrow_mut().push(name.to_string()),
        |entry: NavEntry<i32>| entry.Content(),
    )
}

fn empty_entries(key: i32) -> NavEntry<i32> {
    NavEntry::new(key, |_| {})
}

/// A list on a state, whose owner the test keeps.
fn state_list<T: 'static>(items: impl IntoIterator<Item = T>) -> OwnedStateList<T> {
    mutable_state_list_from(items)
}

#[composable(non_restartable)]
fn ShowLast<T: Clone + 'static>(entries: Vec<NavEntry<T>>) {
    if let Some(entry) = entries.last() {
        entry.Content();
    }
}

thread_local! {
    static SEEN: RefCell<Vec<Vec<NavEntry<i32>>>> = const { RefCell::new(Vec::new()) };
}

fn seen() -> Vec<Vec<NavEntry<i32>>> {
    SEEN.with_borrow(Clone::clone)
}

/// What's the same at another place: the entry and the decorators.
#[composable(non_restartable)]
fn Decorating(
    stack: compose_runtime::snapshots::SnapshotStateList<i32>,
    decorators: ImmutableList<NavEntryDecorator<i32>>,
    entry: NavEntry<i32>,
) {
    let entries = remember_decorated_nav_entries!(
        stack,
        decorators,
        entry_provider = move |_: i32| entry.clone()
    );
    SEEN.with_borrow_mut(|seen| seen.push(entries));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decoratedEntriesRemembered
#[test]
fn decorated_entries_stay_through_a_recomposition() {
    SEEN.with_borrow_mut(Vec::clear);
    let (state, owner) = (mutable_state_of(2), state_list([1]));
    let (read, stack) = (state.handle(), owner.handle());
    let mut ui = compose_test(move || {
        let value = read.get();
        assert_ne!(value, 1, "should not reach this branch");
        // A decorator made at each pass, as Kotlin's `listOf(NavEntryDecorator { … })` in a
        // composable: what a scene's `==` relies on not to change (D1402).
        let entries = remember_decorated_nav_entries!(
            stack,
            vec![NavEntryDecorator::new(|_: NavEntry<i32>| {})],
            entry_provider = empty_entries
        );
        SEEN.with_borrow_mut(|seen| seen.push(entries));
    });
    ui.wait_for_idle();
    state.set(3);
    ui.wait_for_idle();
    let seen = seen();
    assert_eq!(seen.len(), 2, "it recomposed");
    assert_eq!(seen[0], seen[1]);
}

#[composable]
fn InTwoBranches(
    state: MutableState<i32>,
    stack: compose_runtime::snapshots::SnapshotStateList<i32>,
    decorators: ImmutableList<NavEntryDecorator<i32>>,
    entry: NavEntry<i32>,
) {
    if state.get() == 1 {
        Decorating(stack, decorators.clone(), entry.clone());
    } else {
        Decorating(stack, decorators.clone(), entry.clone());
    }
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decoratedEntriesRecreated
#[test]
fn another_place_decorates_anew() {
    SEEN.with_borrow_mut(Vec::clear);
    let (state, owner) = (mutable_state_of(1), state_list([1]));
    let (read, stack) = (state.handle(), owner.handle());
    let decorators: ImmutableList<_> = vec![NavEntryDecorator::new(|_: NavEntry<i32>| {})].into();
    let entry = empty_entries(1);
    let mut ui =
        compose_test(move || InTwoBranches!(read, stack, decorators.clone(), entry.clone()));
    ui.wait_for_idle();
    state.set(2);
    ui.wait_for_idle();
    let seen = seen();
    assert_eq!(seen.len(), 2);
    assert_ne!(seen[0], seen[1], "nothing's changed but the place");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decorator_called
#[test]
fn a_decorator_wraps_what_s_shown() {
    let called = Rc::new(Cell::new(false));
    let marked = called.clone();
    let decorator = NavEntryDecorator::new(move |_: NavEntry<&'static str>| marked.set(true));
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            vec!["something"],
            vec![decorator.clone()],
            entry_provider = |key: &'static str| NavEntry::new(key, |_| {})
        );
        ShowLast(entries);
    });
    ui.wait_for_idle();
    assert!(called.get());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decorator_calledOnce
#[test]
fn a_decorator_given_twice_wraps_once() {
    let count = Rc::new(Cell::new(0));
    let decorator = counting(&count);
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            vec!["something"],
            vec![decorator.clone(), decorator.clone()],
            entry_provider = |key: &'static str| NavEntry::new(key, |_| {})
        );
        ShowLast(entries);
    });
    ui.wait_for_idle();
    assert_eq!(count.get(), 1);
}

/// A decorator logging its call as `name`, then showing the entry.
fn call_logging(name: &'static str, log: &Log) -> NavEntryDecorator<&'static str> {
    let log = log.clone();
    NavEntryDecorator::new(move |entry: NavEntry<&'static str>| {
        log.borrow_mut().push(name.to_string());
        entry.Content();
    })
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decorator_nestedCallOrder
#[test]
fn the_first_decorator_goes_outermost() {
    let log = Log::default();
    let (outer, inner) = (call_logging("outer", &log), call_logging("inner", &log));
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            vec!["something"],
            vec![outer.clone(), inner.clone()],
            entry_provider = |key: &'static str| NavEntry::new(key, |_| {})
        );
        ShowLast(entries);
    });
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["outer", "inner"]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decorator_onPop_order
#[test]
fn the_innermost_decorator_hears_of_a_pop_first() {
    let log = Log::default();
    let (outer, inner) = (pop_logging("outer", &log), pop_logging("inner", &log));
    let owner = state_list([1, 2]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            stack,
            vec![outer.clone(), inner.clone()],
            entry_provider = empty_entries
        );
        ShowLast(entries);
    });
    ui.wait_for_idle();
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["inner", "outer"]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decorator_onPop_neverRenderedEntries
#[test]
fn an_entry_never_shown_is_popped_too() {
    let (popped, rendered) = (Log::default(), Log::default());
    let held = popped.clone();
    let decorator = NavEntryDecorator::with_on_pop(
        move |key| held.borrow_mut().push(key.downcast_ref::<String>().unwrap().clone()),
        |entry: NavEntry<&'static str>| entry.Content(),
    );
    let owner = state_list(["first", "second", "third"]);
    let stack = owner.handle();
    let shown = rendered.clone();
    let mut ui = compose_test(move || {
        let shown = shown.clone();
        let entries = remember_decorated_nav_entries!(
            stack,
            vec![decorator.clone()],
            entry_provider = move |key: &'static str| {
                let shown = shown.clone();
                NavEntry::new(key, move |key| shown.borrow_mut().push(key.to_string()))
                    .with_content_key(key)
            }
        );
        ShowLast(entries);
    });
    ui.wait_for_idle();
    assert_eq!(log_of(&rendered), ["third"]);
    assert!(popped.borrow().is_empty());
    stack.remove(2);
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(log_of(&rendered), ["third", "first"]);
    assert_eq!(log_of(&popped), ["third", "second"]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decorator_onPop_newlyAddedDecorator
#[test]
fn a_decorator_added_since_hears_of_a_pop() {
    let log = Log::default();
    let (one, two) = (pop_logging("decorator1", &log), pop_logging("decorator2", &log));
    let (stack_owner, decorators_owner) = (state_list([1, 2]), state_list([one]));
    let (stack, decorators) = (stack_owner.handle(), decorators_owner.handle());
    let mut ui = compose_test(move || {
        let entries =
            remember_decorated_nav_entries!(stack, decorators, entry_provider = empty_entries);
        ShowLast(entries);
    });
    ui.wait_for_idle();
    assert!(log.borrow().is_empty());
    decorators.push(two);
    ui.wait_for_idle();
    assert!(log.borrow().is_empty());
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["decorator2", "decorator1"]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decorator_noOnPop_removedDecorator
#[test]
fn a_decorator_removed_since_hears_of_no_pop() {
    let log = Log::default();
    let (one, two) = (pop_logging("decorator1", &log), pop_logging("decorator2", &log));
    let (stack_owner, decorators_owner) = (state_list([1, 2]), state_list([one, two.clone()]));
    let (stack, decorators) = (stack_owner.handle(), decorators_owner.handle());
    let mut ui = compose_test(move || {
        let entries =
            remember_decorated_nav_entries!(stack, decorators, entry_provider = empty_entries);
        ShowLast(entries);
    });
    ui.wait_for_idle();
    decorators.remove_item(&two);
    ui.wait_for_idle();
    assert!(log.borrow().is_empty());
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["decorator1"]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decorator_noOnPop_atomicRemoveDecoratorAndPopEntry
#[test]
fn a_decorator_removed_with_the_pop_hears_of_nothing() {
    let log = Log::default();
    let (one, two) = (pop_logging("decorator1", &log), pop_logging("decorator2", &log));
    let (stack_owner, decorators_owner) = (state_list([1, 2]), state_list([one, two.clone()]));
    let (stack, decorators) = (stack_owner.handle(), decorators_owner.handle());
    let mut ui = compose_test(move || {
        let entries =
            remember_decorated_nav_entries!(stack, decorators, entry_provider = empty_entries);
        ShowLast(entries);
    });
    ui.wait_for_idle();
    decorators.remove_item(&two);
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["decorator1"]);
}

/// A decorator logging the content key of what it wraps, then showing it.
fn key_logging(log: &Log) -> NavEntryDecorator<&'static str> {
    let log = log.clone();
    NavEntryDecorator::new(move |entry: NavEntry<&'static str>| {
        log.borrow_mut().push(entry.content_key().downcast_ref::<String>().unwrap().clone());
        entry.Content();
    })
}

fn keyed_by_itself(key: &'static str) -> NavEntry<&'static str> {
    NavEntry::new(key, |_| {}).with_content_key(key)
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decorator_wrapAddedEntry
#[test]
fn a_decorator_wraps_an_entry_added_since() {
    let log = Log::default();
    let decorator = key_logging(&log);
    let owner = state_list(["first"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            stack,
            vec![decorator.clone()],
            entry_provider = keyed_by_itself
        );
        ShowLast(entries);
    });
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["first"]);
    stack.push("second");
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["first", "second"]);
}

/// The entries, then what they were redecorated into.
type Redecorated = (Vec<NavEntry<i32>>, Vec<NavEntry<i32>>);

thread_local! {
    static REDECORATED: RefCell<Vec<Redecorated>> = const { RefCell::new(Vec::new()) };
}

#[composable(non_restartable)]
fn Redecorating(
    stack: compose_runtime::snapshots::SnapshotStateList<i32>,
    decorators: ImmutableList<NavEntryDecorator<i32>>,
    entry: NavEntry<i32>,
) {
    let initial = remember_decorated_nav_entries!(
        stack,
        decorators.clone(),
        entry_provider = move |_: i32| entry.clone()
    );
    let redecorated = remember_decorated_nav_entries_from!(initial.clone(), decorators);
    REDECORATED.with_borrow_mut(|seen| seen.push((initial, redecorated)));
}

fn redecorated_seen() -> Vec<Redecorated> {
    REDECORATED.with_borrow(Clone::clone)
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, redecoratedEntriesRemembered
#[test]
fn redecorated_entries_stay_through_a_recomposition() {
    REDECORATED.with_borrow_mut(Vec::clear);
    let (state, owner) = (mutable_state_of(2), state_list([1]));
    let (read, stack) = (state.handle(), owner.handle());
    let mut ui = compose_test(move || {
        let value = read.get();
        assert_ne!(value, 1, "should not reach this branch");
        let initial = remember_decorated_nav_entries!(
            stack,
            vec![NavEntryDecorator::new(|_: NavEntry<i32>| {})],
            entry_provider = empty_entries
        );
        let redecorated = remember_decorated_nav_entries_from!(
            initial.clone(),
            vec![NavEntryDecorator::new(|_: NavEntry<i32>| {})]
        );
        REDECORATED.with_borrow_mut(|seen| seen.push((initial, redecorated)));
    });
    ui.wait_for_idle();
    state.set(3);
    ui.wait_for_idle();
    let seen = redecorated_seen();
    assert_eq!(seen.len(), 2, "it recomposed");
    assert_eq!(seen[0], seen[1]);
}

#[composable]
fn RedecoratingInTwoBranches(
    state: MutableState<i32>,
    stack: compose_runtime::snapshots::SnapshotStateList<i32>,
    decorators: ImmutableList<NavEntryDecorator<i32>>,
    entry: NavEntry<i32>,
) {
    if state.get() == 1 {
        Redecorating(stack, decorators.clone(), entry.clone());
    } else {
        Redecorating(stack, decorators.clone(), entry.clone());
    }
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, redecoratedEntriesRecreated
#[test]
fn another_place_redecorates_anew() {
    REDECORATED.with_borrow_mut(Vec::clear);
    let (state, owner) = (mutable_state_of(1), state_list([1]));
    let (read, stack) = (state.handle(), owner.handle());
    let decorators: ImmutableList<_> = vec![NavEntryDecorator::new(|_: NavEntry<i32>| {})].into();
    let entry = empty_entries(1);
    let mut ui = compose_test(move || {
        RedecoratingInTwoBranches!(read, stack, decorators.clone(), entry.clone());
    });
    ui.wait_for_idle();
    state.set(2);
    ui.wait_for_idle();
    let seen = redecorated_seen();
    assert_eq!(seen.len(), 2);
    assert_ne!(seen[0].0, seen[1].0);
    assert_ne!(seen[0].1, seen[1].1);
}

/// The entries of `keys`, with `initial` and then `additional` around them, as the test's
/// `WithRedecoratedNavEntries`.
#[composable(non_restartable)]
fn ShowRedecorated<T: Clone + PartialEq + 'static>(
    keys: Vec<T>,
    initial: Vec<NavEntryDecorator<T>>,
    additional: Vec<NavEntryDecorator<T>>,
    entry_provider: impl Fn(T) -> NavEntry<T> + 'static,
) {
    let entries = remember_decorated_nav_entries!(keys, initial, entry_provider = entry_provider);
    ShowLast(remember_decorated_nav_entries_from!(entries, additional));
}

fn something(key: &'static str) -> NavEntry<&'static str> {
    NavEntry::new(key, |_| {})
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, redecorator_called
#[test]
fn a_redecorator_and_a_decorator_both_wrap() {
    let log = Log::default();
    let (decorator, redecorator) = (call_logging("decorator", &log), call_logging("again", &log));
    let mut ui = compose_test(move || {
        ShowRedecorated(
            vec!["something"],
            vec![decorator.clone()],
            vec![redecorator.clone()],
            something.into_callback(),
        );
    });
    ui.wait_for_idle();
    assert!(log.borrow().contains(&"decorator".to_string()));
    assert!(log.borrow().contains(&"again".to_string()));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, redecorator_calledOnce
#[test]
fn a_redecorator_and_a_decorator_each_wrap_once() {
    let log = Log::default();
    let (decorator, redecorator) = (call_logging("decorator", &log), call_logging("again", &log));
    let mut ui = compose_test(move || {
        ShowRedecorated(
            vec!["something"],
            vec![decorator.clone()],
            vec![redecorator.clone()],
            something.into_callback(),
        );
    });
    ui.wait_for_idle();
    let log = log_of(&log);
    assert_eq!(log.iter().filter(|name| *name == "decorator").count(), 1);
    assert_eq!(log.iter().filter(|name| *name == "again").count(), 1);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, redecorator_nestedCallOrder
#[test]
fn a_redecorator_goes_outside_the_decorators() {
    let log = Log::default();
    let (decorator, redecorator) = (call_logging("inner", &log), call_logging("outer", &log));
    let mut ui = compose_test(move || {
        ShowRedecorated(
            vec!["something"],
            vec![decorator.clone()],
            vec![redecorator.clone()],
            something.into_callback(),
        );
    });
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["outer", "inner"]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, redecorator_onPop_order
#[test]
fn a_redecorator_hears_of_a_pop_before_a_decorator() {
    let log = Log::default();
    let (decorator, redecorator) = (pop_logging("decorator", &log), pop_logging("again", &log));
    let owner = state_list([1, 2]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            stack,
            vec![decorator.clone()],
            entry_provider = empty_entries
        );
        ShowLast(remember_decorated_nav_entries_from!(entries, vec![redecorator.clone()]));
    });
    ui.wait_for_idle();
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["again", "decorator"]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, redecorator_wrapAddedEntry
#[test]
fn a_redecorator_wraps_an_entry_added_since() {
    let log = Log::default();
    let decorator = NavEntryDecorator::new(|entry: NavEntry<&'static str>| entry.Content());
    let redecorator = key_logging(&log);
    let owner = state_list(["first"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            stack,
            vec![decorator.clone()],
            entry_provider = keyed_by_itself
        );
        ShowLast(remember_decorated_nav_entries_from!(entries, vec![redecorator.clone()]));
    });
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["first"]);
    stack.push("second");
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["first", "second"]);
}

fn counting(count: &Rc<Cell<u32>>) -> NavEntryDecorator<&'static str> {
    let count = count.clone();
    NavEntryDecorator::new(move |entry: NavEntry<&'static str>| {
        count.set(count.get() + 1);
        entry.Content();
    })
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, entry_reWrappedWithNewlyAddedDecorator
#[test]
fn an_entry_is_wrapped_again_with_a_decorator_added() {
    let (count1, count2) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (decorator1, decorator2) = (counting(&count1), counting(&count2));
    let owner = state_list([decorator1]);
    let decorators = owner.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            vec!["something"],
            decorators,
            entry_provider = something
        );
        ShowLast(entries);
    });
    ui.wait_for_idle();
    assert_eq!((count1.get(), count2.get()), (1, 0));
    decorators.push(decorator2);
    ui.wait_for_idle();
    assert_eq!((count1.get(), count2.get()), (2, 1));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, entry_reWrappedWithRemovedDecorator
#[test]
fn an_entry_is_wrapped_again_with_a_decorator_removed() {
    let (count1, count2) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (decorator1, decorator2) = (counting(&count1), counting(&count2));
    let owner = state_list([decorator1.clone(), decorator2]);
    let decorators = owner.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            vec!["something"],
            decorators,
            entry_provider = something
        );
        ShowLast(entries);
    });
    ui.wait_for_idle();
    assert_eq!((count1.get(), count2.get()), (1, 1));
    decorators.remove_item(&decorator1);
    ui.wait_for_idle();
    assert_eq!((count1.get(), count2.get()), (1, 2));
}

/// Kotlin's `DataClass`.
#[derive(Clone, Copy, PartialEq, Hash, Debug)]
struct Data(i32);

/// A provider keying entries by the number in their key, as `entry<DataClass>({ it.arg }) {}`.
fn by_number() -> impl Fn(Data) -> NavEntry<Data> + Clone + 'static {
    entry_provider(|scope| {
        scope.entry_of::<Data, _>(|_| {}).with_content_key_of(|key: &Data| key.0);
    })
}

type States = Rc<RefCell<BTreeSet<i32>>>;

fn states_of(states: &States) -> Vec<i32> {
    states.borrow().iter().copied().collect()
}

/// A decorator keeping a state for each content key while it's on the stack, as the tests' map.
fn keeping(states: &States) -> NavEntryDecorator<Data> {
    let (put, removed) = (states.clone(), states.clone());
    NavEntryDecorator::with_on_pop(
        move |key| {
            removed.borrow_mut().remove(key.downcast_ref::<i32>().unwrap());
        },
        move |entry: NavEntry<Data>| {
            put.borrow_mut().insert(*entry.content_key().downcast_ref::<i32>().unwrap());
            entry.Content();
        },
    )
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decoratorState_differentContentKeySeparatesState
#[test]
fn other_content_keys_have_their_own_state() {
    let states = States::default();
    let decorator = keeping(&states);
    let owner = state_list([Data(1)]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            stack,
            vec![decorator.clone()],
            entry_provider = by_number()
        );
        ShowLast(entries);
    });
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [1]);
    stack.push(Data(2));
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [1, 2]);
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [1]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, decoratorState_sameContentKeySharedState
#[test]
fn entries_sharing_a_content_key_aren_t_popped_one_by_one() {
    let popped = Rc::new(Cell::new(false));
    let marked = popped.clone();
    let decorator = NavEntryDecorator::with_on_pop(
        move |_| marked.set(true),
        |entry: NavEntry<i32>| entry.Content(),
    );
    let (entry1, entry2) = (
        NavEntry::new(1, |_| {}).with_content_key("sameKey"),
        NavEntry::new(2, |_| {}).with_content_key("sameKey"),
    );
    assert_eq!(entry1.content_key(), entry2.content_key());
    let owner = state_list([1, 2]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let (entry1, entry2) = (entry1.clone(), entry2.clone());
        let entries = remember_decorated_nav_entries!(
            stack,
            vec![decorator.clone()],
            entry_provider = move |key: i32| if key == 1 { entry1.clone() } else { entry2.clone() }
        );
        ShowLast(entries);
    });
    ui.wait_for_idle();
    stack.remove(1);
    ui.wait_for_idle();
    assert!(!popped.get());
}

/// With a back stack that's a plain list chosen by a state, and a decorator made at each pass.
fn immutable_stacks(
    states: &States,
    first: Vec<Data>,
    second: Vec<Data>,
) -> (ComposeTestRule, OwnedState<i32>) {
    let state = mutable_state_of(1);
    let (read, states) = (state.handle(), states.clone());
    let ui = compose_test(move || {
        let stack = if read.get() == 1 { first.clone() } else { second.clone() };
        let entries = remember_decorated_nav_entries!(
            stack,
            vec![keeping(&states)],
            entry_provider = by_number()
        );
        ShowLast(entries);
    });
    (ui, state)
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, singleBackStack_immutableBackStack_navigate
#[test]
fn a_new_list_with_one_more_key_navigates() {
    let states = States::default();
    let (mut ui, state) = immutable_stacks(&states, vec![Data(1)], vec![Data(1), Data(2)]);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [1]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [1, 2]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, singleBackStack_immutableBackStack_pop
#[test]
fn a_new_list_with_one_key_less_pops() {
    let states = States::default();
    let (mut ui, state) = immutable_stacks(&states, vec![Data(1), Data(2)], vec![Data(1)]);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [2]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [1]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, singleBackStack_immutableBackStackNavigateDupKey_sharedState
#[test]
fn a_new_list_adding_a_key_again_shares_its_state() {
    let states = States::default();
    let (mut ui, state) =
        immutable_stacks(&states, vec![Data(1), Data(2)], vec![Data(1), Data(2), Data(2)]);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [2]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [2]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, singleBackStack_immutableBackStackPopDupKey_statePreserved
#[test]
fn a_new_list_without_a_duplicate_keeps_its_state() {
    let (states, popped) = (States::default(), Rc::new(Cell::new(false)));
    let state = mutable_state_of(1);
    let (read, marked, held) = (state.handle(), popped.clone(), states.clone());
    let mut ui = compose_test(move || {
        let stack =
            if read.get() == 1 { vec![Data(1), Data(2), Data(2)] } else { vec![Data(1), Data(2)] };
        let (marked, removed, put) = (marked.clone(), held.clone(), held.clone());
        let decorator = NavEntryDecorator::with_on_pop(
            move |key| {
                removed.borrow_mut().remove(key.downcast_ref::<i32>().unwrap());
                marked.set(true);
            },
            move |entry: NavEntry<Data>| {
                put.borrow_mut().insert(*entry.content_key().downcast_ref::<i32>().unwrap());
                entry.Content();
            },
        );
        let entries =
            remember_decorated_nav_entries!(stack, vec![decorator], entry_provider = by_number());
        ShowLast(entries);
    });
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [2]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [2]);
    assert!(!popped.get());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, singleBackStack_immutableBackStackPopAllDupKeys_stateCleared
#[test]
fn a_new_list_without_any_duplicate_forgets_its_state() {
    let states = States::default();
    let (mut ui, state) = immutable_stacks(&states, vec![Data(1), Data(2), Data(2)], vec![Data(1)]);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [2]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [1]);
}

/// With a back stack and a decorator kept outside.
fn hoisted(
    states: &States,
    keys: impl IntoIterator<Item = Data>,
) -> (ComposeTestRule, OwnedStateList<Data>) {
    let owner = state_list(keys);
    let stack = owner.handle();
    let decorators = vec![keeping(states)];
    let provider = Rc::new(by_number());
    let ui = compose_test(move || {
        let provider = provider.clone();
        let entries = remember_decorated_nav_entries!(
            stack,
            decorators.clone(),
            entry_provider = move |key: Data| provider(key)
        );
        ShowLast(entries);
    });
    (ui, owner)
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, singleBackStack_hoistedStatesNavigateDupKey_sharedState
#[test]
fn a_key_added_again_shares_its_state() {
    let states = States::default();
    let (mut ui, stack) = hoisted(&states, [Data(1), Data(2)]);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [2]);
    stack.push(Data(2));
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [2]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, singleBackStack_hoistedStatesPopDupKey_statePreserved
#[test]
fn a_duplicate_popped_keeps_its_state() {
    let states = States::default();
    let (mut ui, stack) = hoisted(&states, [Data(1), Data(2), Data(2)]);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [2]);
    stack.remove(2);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [2]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, singleBackStack_hoistedStatesPopAllDupKeys_stateCleared
#[test]
fn every_duplicate_popped_forgets_its_state() {
    let states = States::default();
    let (mut ui, stack) = hoisted(&states, [Data(1), Data(2), Data(2)]);
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [2]);
    stack.clear();
    stack.push(Data(1));
    ui.wait_for_idle();
    assert_eq!(states_of(&states), [1]);
}

/// With decorators chosen by a state, keeping apart.
fn swapped_decorators(
    previous: &States,
    new: &States,
) -> (ComposeTestRule, OwnedStateList<Data>, OwnedState<i32>) {
    let (owner, state) = (state_list([Data(1), Data(2)]), mutable_state_of(1));
    let (stack, read) = (owner.handle(), state.handle());
    let (first, second) = (vec![keeping(previous)], vec![keeping(new)]);
    let provider = Rc::new(by_number());
    let ui = compose_test(move || {
        let decorators = if read.get() == 1 { first.clone() } else { second.clone() };
        let provider = provider.clone();
        let entries = remember_decorated_nav_entries!(
            stack,
            decorators,
            entry_provider = move |key: Data| provider(key)
        );
        ShowLast(entries);
    });
    (ui, owner, state)
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, singleBackStack_swapDecorators_previousStatePreserved
#[test]
fn decorators_swapped_leave_the_last_ones_state() {
    let (previous, new) = (States::default(), States::default());
    let (mut ui, _stack, state) = swapped_decorators(&previous, &new);
    ui.wait_for_idle();
    assert_eq!(states_of(&previous), [2]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(states_of(&previous), [2]);
    assert_eq!(states_of(&new), [2]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, singleBackStack_swapDecoratorsThenPop_stateCleared
#[test]
fn a_pop_goes_to_the_decorators_swapped_in() {
    let (previous, new) = (States::default(), States::default());
    let (mut ui, stack, state) = swapped_decorators(&previous, &new);
    ui.wait_for_idle();
    assert_eq!(states_of(&previous), [2]);
    state.set(2);
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(states_of(&previous), [2]);
    assert_eq!(states_of(&new), [1]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, singleBackStack_swapDecoratorStateThenPop_stateCleared
#[test]
fn a_pop_reaches_the_state_a_decorator_reads_now() {
    let (previous, new) = (States::default(), States::default());
    let current: Rc<RefCell<States>> = Rc::new(RefCell::new(previous.clone()));
    let (put, removed) = (current.clone(), current.clone());
    let decorators = vec![NavEntryDecorator::with_on_pop(
        move |key: ContentKey| {
            removed.borrow().borrow_mut().remove(key.downcast_ref::<i32>().unwrap());
        },
        move |entry: NavEntry<Data>| {
            let key = *entry.content_key().downcast_ref::<i32>().unwrap();
            put.borrow().borrow_mut().insert(key);
            entry.Content();
        },
    )];
    let (owner, state) = (state_list([Data(1), Data(2)]), mutable_state_of(1));
    let (stack, read) = (owner.handle(), state.handle());
    let (first, second, held) = (previous.clone(), new.clone(), current.clone());
    let provider = Rc::new(by_number());
    let mut ui = compose_test(move || {
        *held.borrow_mut() = if read.get() == 1 { first.clone() } else { second.clone() };
        let provider = provider.clone();
        let entries = remember_decorated_nav_entries!(
            stack,
            decorators.clone(),
            entry_provider = move |key: Data| provider(key)
        );
        ShowLast(entries);
    });
    ui.wait_for_idle();
    assert_eq!(states_of(&previous), [2]);
    state.set(2);
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(states_of(&previous), [2]);
    assert_eq!(states_of(&new), [1]);
}

fn key_of(key: i32) -> ContentKey {
    empty_entries(key).content_key().clone()
}

type Keys = Rc<RefCell<Vec<ContentKey>>>;

/// A decorator noting each content key it's given, showing nothing, and forgetting one popped.
fn noting(keys: &Keys) -> NavEntryDecorator<i32> {
    let (noted, removed) = (keys.clone(), keys.clone());
    NavEntryDecorator::with_on_pop(
        move |key| {
            let mut keys = removed.borrow_mut();
            if let Some(index) = keys.iter().position(|kept| *kept == key) {
                keys.remove(index);
            }
        },
        move |entry: NavEntry<i32>| noted.borrow_mut().push(entry.content_key().clone()),
    )
}

fn provider_of(keys: &'static [i32]) -> impl Fn(i32) -> NavEntry<i32> + Clone + 'static {
    entry_provider(move |scope| {
        for key in keys {
            scope.entry(*key, |_| {});
        }
    })
}

/// Two back stacks, with a decorator made at each pass, and what `choose` makes of their entries
/// showing its last. The keys their providers know are `known1` and `known2`.
fn two_stacks(
    (stack1, known1): (&[i32], &'static [i32]),
    (stack2, known2): (&[i32], &'static [i32]),
    keys: (&Keys, &Keys),
    choose: impl Fn(i32, Vec<NavEntry<i32>>, Vec<NavEntry<i32>>) -> Vec<NavEntry<i32>> + 'static,
) -> (ComposeTestRule, OwnedStateList<i32>, OwnedStateList<i32>, OwnedState<i32>) {
    let (owner1, owner2) = (state_list(stack1.to_vec()), state_list(stack2.to_vec()));
    let state = mutable_state_of(1);
    let (first, second, read) = (owner1.handle(), owner2.handle(), state.handle());
    let (keys1, keys2) = (keys.0.clone(), keys.1.clone());
    let ui = compose_test(move || {
        let entries1 = remember_decorated_nav_entries!(
            first,
            vec![noting(&keys1)],
            entry_provider = provider_of(known1)
        );
        let entries2 = remember_decorated_nav_entries!(
            second,
            vec![noting(&keys2)],
            entry_provider = provider_of(known2)
        );
        ShowLast(choose(read.get(), entries1, entries2));
    });
    (ui, owner1, owner2, state)
}

fn one_or_other(
    state: i32,
    one: Vec<NavEntry<i32>>,
    other: Vec<NavEntry<i32>>,
) -> Vec<NavEntry<i32>> {
    if state == 1 { one } else { other }
}

fn one_then_both(
    state: i32,
    one: Vec<NavEntry<i32>>,
    other: Vec<NavEntry<i32>>,
) -> Vec<NavEntry<i32>> {
    if state == 1 { one } else { one.into_iter().chain(other).collect() }
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, multipleBackStack_swapDecoratedEntries_previousStatePreserved
#[test]
fn back_stacks_swapped_leave_the_last_ones_state() {
    let (keys1, keys2) = (Keys::default(), Keys::default());
    let (mut ui, _one, _two, state) =
        two_stacks((&[1], &[1]), (&[2], &[2]), (&keys1, &keys2), one_or_other);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
    assert_eq!(*keys2.borrow(), [key_of(2)]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, multipleBackStack_swapDecoratedEntriesThenPop_stateCleared
#[test]
fn back_stacks_swapped_each_pop_their_own() {
    let (keys1, keys2) = (Keys::default(), Keys::default());
    let (mut ui, one, two, state) =
        two_stacks((&[1, 2], &[1, 2]), (&[3, 4], &[3, 4]), (&keys1, &keys2), one_or_other);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(2)]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(*keys2.borrow(), [key_of(4)]);
    two.remove(1);
    ui.wait_for_idle();
    assert_eq!(*keys2.borrow(), [key_of(3)]);
    state.set(1);
    one.remove(1);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, multipleBackStack_concatenateDecoratedEntries
#[test]
fn entries_of_two_stacks_can_be_concatenated() {
    let (keys1, keys2) = (Keys::default(), Keys::default());
    let (mut ui, _one, _two, state) =
        two_stacks((&[1], &[1]), (&[2], &[2]), (&keys1, &keys2), |state, one, other| match state {
            1 => one,
            2 => other,
            3 => one.into_iter().chain(other).collect(),
            _ => other.into_iter().chain(one).collect(),
        });
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
    assert_eq!(*keys2.borrow(), [key_of(2)]);
    state.set(3);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
    assert_eq!(*keys2.borrow(), [key_of(2), key_of(2)], "shown again");
    state.set(4);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1), key_of(1)], "shown again");
    assert_eq!(*keys2.borrow(), [key_of(2), key_of(2)]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, multipleBackStack_concatenateDecoratedEntries_navigateOnFirstStack
#[test]
fn a_key_added_to_the_first_of_two_stacks_isn_t_shown() {
    let (keys1, keys2) = (Keys::default(), Keys::default());
    let (mut ui, one, _two, state) =
        two_stacks((&[1], &[1, 3]), (&[2], &[2]), (&keys1, &keys2), one_then_both);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
    assert_eq!(*keys2.borrow(), [key_of(2)]);
    one.push(3);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
    assert_eq!(*keys2.borrow(), [key_of(2), key_of(2)], "the last shown again");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, multipleBackStack_concatenateDecoratedEntries_navigateOnLastStack
#[test]
fn a_key_added_to_the_last_of_two_stacks_is_shown() {
    let (keys1, keys2) = (Keys::default(), Keys::default());
    let (mut ui, _one, two, state) =
        two_stacks((&[1], &[1]), (&[2], &[2, 3]), (&keys1, &keys2), one_then_both);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(*keys2.borrow(), [key_of(2)]);
    two.push(3);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
    assert_eq!(*keys2.borrow(), [key_of(2), key_of(3)]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, multipleBackStack_concatenateDecoratedEntries_popOnFirstStack
#[test]
fn a_pop_on_the_first_of_two_stacks_forgets_without_showing() {
    let (keys1, keys2) = (Keys::default(), Keys::default());
    let (mut ui, one, _two, state) =
        two_stacks((&[1, 2], &[1, 2]), (&[3], &[3]), (&keys1, &keys2), one_then_both);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(2)]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(2)]);
    assert_eq!(*keys2.borrow(), [key_of(3)]);
    one.remove(1);
    ui.wait_for_idle();
    assert!(keys1.borrow().is_empty());
    assert_eq!(*keys2.borrow(), [key_of(3), key_of(3)], "the last shown again");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, multipleBackStack_concatenateDecoratedEntries_popOnLastStack
#[test]
fn a_pop_on_the_last_of_two_stacks_shows_the_one_below() {
    let (keys1, keys2) = (Keys::default(), Keys::default());
    let (mut ui, _one, two, state) =
        two_stacks((&[1], &[1]), (&[2, 3], &[2, 3]), (&keys1, &keys2), one_then_both);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
    state.set(2);
    ui.wait_for_idle();
    assert_eq!(*keys2.borrow(), [key_of(3)]);
    two.remove(1);
    ui.wait_for_idle();
    assert_eq!(*keys1.borrow(), [key_of(1)]);
    assert_eq!(*keys2.borrow(), [key_of(2)]);
}

/// A stack of a state list, and one that's a plain list chosen by a state.
fn with_plain_stack(
    (first, second): (Vec<i32>, Vec<i32>),
    keys: &Keys,
) -> (ComposeTestRule, OwnedState<i32>, OwnedState<i32>) {
    let (stacks, stack) = (mutable_state_of(1), mutable_state_of(1));
    let (which, chosen, noted) = (stacks.handle(), stack.handle(), keys.clone());
    let plain = state_list([1]);
    let plain_handle = plain.handle();
    let ui = compose_test(move || {
        let _keep = &plain;
        let entries1 = remember_decorated_nav_entries!(
            plain_handle,
            Vec::new(),
            entry_provider = provider_of(&[1])
        );
        let second_stack = if chosen.get() == 1 { first.clone() } else { second.clone() };
        let entries2 = remember_decorated_nav_entries!(
            second_stack,
            vec![noting(&noted)],
            entry_provider = provider_of(&[2, 3])
        );
        ShowLast(one_or_other(which.get(), entries1, entries2));
    });
    (ui, stacks, stack)
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, multipleBackStack_immutableBackStack_navigate
#[test]
fn a_new_list_for_the_stack_swapped_in_navigates() {
    let keys = Keys::default();
    let (mut ui, stacks, stack) = with_plain_stack((vec![2], vec![2, 3]), &keys);
    ui.wait_for_idle();
    stacks.set(2);
    ui.wait_for_idle();
    assert_eq!(*keys.borrow(), [key_of(2)]);
    stack.set(2);
    ui.wait_for_idle();
    assert_eq!(*keys.borrow(), [key_of(2), key_of(3)]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt, multipleBackStack_immutableBackStack_pop
#[test]
fn a_new_list_for_the_stack_swapped_in_pops() {
    let keys = Keys::default();
    let (mut ui, stacks, stack) = with_plain_stack((vec![2, 3], vec![2]), &keys);
    ui.wait_for_idle();
    stacks.set(2);
    ui.wait_for_idle();
    assert_eq!(*keys.borrow(), [key_of(3)]);
    stack.set(2);
    ui.wait_for_idle();
    assert_eq!(*keys.borrow(), [key_of(2)]);
}

/// A display keeps a popped entry shown while it animates out: what's kept for it stays until its
/// content leaves, which the pop's frame doesn't do, and it's told once.
#[test]
fn an_entry_popped_while_shown_is_told_when_it_leaves() {
    let log = Log::default();
    let popped = log.clone();
    let decorator = NavEntryDecorator::with_on_pop(
        move |_| popped.borrow_mut().push("popped".to_string()),
        |entry: NavEntry<i32>| entry.Content(),
    );
    let (owner, leaving) = (state_list([1, 2]), mutable_state_of(false));
    let (stack, left) = (owner.handle(), leaving.handle());
    let kept: Rc<RefCell<Option<NavEntry<i32>>>> = Rc::default();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            stack,
            vec![decorator.clone()],
            entry_provider = empty_entries
        );
        KeepingTheLast(entries, kept.clone(), left);
    });
    ui.wait_for_idle();
    stack.remove(1);
    ui.wait_for_idle();
    assert!(log.borrow().is_empty(), "still shown");
    leaving.set(true);
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["popped"]);
}

/// Shows the first entry it saw at the end of the list until `left`.
#[composable(non_restartable)]
fn KeepingTheLast(
    entries: Vec<NavEntry<i32>>,
    kept: Rc<RefCell<Option<NavEntry<i32>>>>,
    left: MutableState<bool>,
) {
    if kept.borrow().is_none() {
        *kept.borrow_mut() = entries.last().cloned();
    }
    let shown = if left.get() { entries.last().cloned() } else { kept.borrow().clone() };
    shown.unwrap().Content();
}

/// Each entry's decoration follows its content key when the stack's order changes.
#[test]
fn a_decorated_entry_stays_when_the_stack_is_reordered() {
    SEEN.with_borrow_mut(Vec::clear);
    let (one, two) = (empty_entries(1), empty_entries(2));
    let owner = state_list([1, 2]);
    let stack = owner.handle();
    let decorators = vec![NavEntryDecorator::new(|_: NavEntry<i32>| {})];
    let mut ui = compose_test(move || {
        let (one, two) = (one.clone(), two.clone());
        let entries = remember_decorated_nav_entries!(
            stack,
            decorators.clone(),
            entry_provider = move |key: i32| if key == 1 { one.clone() } else { two.clone() }
        );
        SEEN.with_borrow_mut(|seen| seen.push(entries));
    });
    ui.wait_for_idle();
    stack.swap(0, 1);
    ui.wait_for_idle();
    let seen = seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0][0], seen[1][1]);
    assert_eq!(seen[0][1], seen[1][0]);
}

/// A push and a pop rebuild every entry of the stack: what's decorated at a place stays equal, a
/// provider's closures being made anew or not, and so what a scene or a lifecycle compares (D1402).
#[test]
fn a_decorated_entry_stays_equal_through_a_push_and_a_pop() {
    SEEN.with_borrow_mut(Vec::clear);
    let owner = state_list([1]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            stack,
            vec![NavEntryDecorator::new(|entry: NavEntry<i32>| entry.Content())],
            entry_provider = |key: i32| NavEntry::new(key, |_| {})
        );
        SEEN.with_borrow_mut(|seen| seen.push(entries));
    });
    ui.wait_for_idle();
    stack.push(2);
    ui.wait_for_idle();
    stack.remove(1);
    ui.wait_for_idle();
    let seen = seen();
    assert_eq!(seen.first().unwrap()[0], seen.last().unwrap()[0]);
    assert_eq!(seen[0][0], seen[1][0], "with the other on top");
    assert_ne!(seen[1][0], seen[1][1]);
}

/// What isn't decorated has no place: a provider's closures made anew are other content. Accepted:
/// `NavDisplay` always decorates, and the DSL's entries share their closure.
#[test]
fn undecorated_entries_of_new_closures_differ_and_the_dsl_s_don_t() {
    assert_ne!(NavEntry::new(1, |_| {}), NavEntry::new(1, |_| {}));
    let provider = entry_provider(|scope| {
        scope.entry(1, |_| {});
    });
    assert_eq!(provider(1), provider(1));
}

/// Decorated again with another decorator, the place's content is the same, and what it shows is
/// the new one's.
#[test]
fn a_place_s_content_follows_what_it_s_decorated_with() {
    let log = Log::default();
    let decorators = state_list([pop_or_show("first", &log)]);
    let handle = decorators.handle();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let kept = seen.clone();
    let mut ui = compose_test(move || {
        let entries =
            remember_decorated_nav_entries!(vec![1], handle, entry_provider = empty_entries);
        kept.borrow_mut().push(entries[0].clone());
        entries[0].Content();
    });
    ui.wait_for_idle();
    handle.set(0, pop_or_show("second", &log));
    ui.wait_for_idle();
    assert_eq!(log_of(&log), ["first", "second"]);
    let seen = seen.borrow();
    assert_eq!(seen.first(), seen.last());
}

fn pop_or_show(name: &'static str, log: &Log) -> NavEntryDecorator<i32> {
    let log = log.clone();
    NavEntryDecorator::new(move |entry: NavEntry<i32>| {
        log.borrow_mut().push(name.to_string());
        entry.Content();
    })
}

thread_local! {
    static NEXT_STATE: Cell<u32> = const { Cell::new(0) };
    static DECORATOR_STATE: Cell<u32> = const { Cell::new(0) };
}

fn remembering_in_decorator() {
    DECORATOR_STATE.set(remember(|| {
        NEXT_STATE.set(NEXT_STATE.get() + 1);
        NEXT_STATE.get()
    }));
}

/// Another decorator's closure at a position starts anew, as Kotlin's lambda, and the same closure's
/// doesn't.
#[test]
fn another_decorator_at_a_position_resets_its_state() {
    NEXT_STATE.set(0);
    fn same_place() -> NavEntryDecorator<i32> {
        NavEntryDecorator::new(|entry: NavEntry<i32>| {
            remembering_in_decorator();
            entry.Content();
        })
    }
    let other = NavEntryDecorator::new(|entry: NavEntry<i32>| {
        remembering_in_decorator();
        entry.Content();
    });
    let owner = state_list([same_place()]);
    let decorators = owner.handle();
    let mut ui = compose_test(move || {
        let entries =
            remember_decorated_nav_entries!(vec![1], decorators, entry_provider = empty_entries);
        entries[0].Content();
    });
    ui.wait_for_idle();
    assert_eq!(DECORATOR_STATE.get(), 1);
    decorators.set(0, same_place());
    ui.wait_for_idle();
    assert_eq!(DECORATOR_STATE.get(), 1, "the same closure, another decorator");
    decorators.set(0, other);
    ui.wait_for_idle();
    assert_eq!(DECORATOR_STATE.get(), 2, "another closure");
}

/// A place's entry follows what it's made of, when entries share a content key and differ in the
/// rest.
#[test]
fn a_decorated_entry_has_its_entry_s_latest_metadata() {
    SEEN.with_borrow_mut(Vec::clear);
    let state = mutable_state_of(1);
    let read = state.handle();
    let mut ui = compose_test(move || {
        let entries = remember_decorated_nav_entries!(
            vec![read.get()],
            vec![NavEntryDecorator::new(|entry: NavEntry<i32>| entry.Content())],
            entry_provider = |key: i32| {
                NavEntry::keyed(key, "shared", |_| {})
                    .with_metadata(NavMetadata::new().with("key", key))
            }
        );
        SEEN.with_borrow_mut(|seen| seen.push(entries));
    });
    ui.wait_for_idle();
    state.set(2);
    ui.wait_for_idle();
    let seen = seen();
    assert_eq!(seen[0][0].metadata().get_named::<i32>("key"), Some(&1));
    assert_eq!(seen[1][0].metadata().get_named::<i32>("key"), Some(&2));
    assert_ne!(seen[0][0], seen[1][0]);
}
