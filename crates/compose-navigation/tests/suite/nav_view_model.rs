//! A view model for each entry, against `lifecycle-viewmodel-navigation3`'s
//! `ViewModelStoreNavEntryDecoratorTest`. `testViewModelSavedStateHandle` waits for the saved-state
//! library (D838). `testViewModelNoSavedStateNavEntryDecorator` is of Android's `SavedStateHandle`
//! needing its registry: there's none, so it works.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_lifecycle::*;
use compose_navigation::*;
use compose_runtime::snapshots::mutable_state_list_from;
use compose_runtime::*;
use compose_test::*;

#[derive(Default)]
struct MyViewModel {
    base: ViewModelBase,
    my_arg: RefCell<&'static str>,
    cleared: Cell<bool>,
}

impl ViewModel for MyViewModel {
    fn base(&self) -> &ViewModelBase {
        &self.base
    }

    fn on_cleared(&self) {
        self.cleared.set(true);
    }
}

type Held = Rc<RefCell<Vec<(&'static str, Rc<MyViewModel>)>>>;

fn view_model_of(held: &Held, key: &str) -> Rc<MyViewModel> {
    held.borrow().iter().rev().find(|(name, _)| *name == key).unwrap().1.clone()
}

/// An entry with a view model, noted by its key.
fn noting(held: &Held) -> impl Fn(&'static str) -> NavEntry<&'static str> + Clone + 'static {
    let held = held.clone();
    move |key| {
        let held = held.clone();
        NavEntry::new(key, move |key| {
            let model = view_model::<MyViewModel>();
            held.borrow_mut().push((key, model));
        })
    }
}

// Compose: lifecycle/lifecycle-viewmodel-navigation3/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/navigation3/ViewModelStoreNavEntryDecoratorTest.kt, testViewModelProvided
#[test]
fn each_entry_has_its_own_view_model() {
    let held = Held::default();
    let provider = noting(&held);
    let mut ui = compose_test(move || {
        let saveable = remember_saveable_state_holder_nav_entry_decorator!();
        let models = remember_view_model_store_nav_entry_decorator!();
        let entries = remember_decorated_nav_entries_from!(
            vec![provider("key1"), provider("key2")],
            vec![saveable, models]
        );
        for entry in &entries {
            entry.Content();
        }
    });
    ui.wait_for_idle();
    *view_model_of(&held, "key1").my_arg.borrow_mut() = "entry1 Arg";
    *view_model_of(&held, "key2").my_arg.borrow_mut() = "entry2 Arg";
    assert_eq!(*view_model_of(&held, "key1").my_arg.borrow(), "entry1 Arg");
    assert_eq!(*view_model_of(&held, "key2").my_arg.borrow(), "entry2 Arg");
    assert!(!Rc::ptr_eq(&view_model_of(&held, "key1"), &view_model_of(&held, "key2")));
}

// Compose: lifecycle/lifecycle-viewmodel-navigation3/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/navigation3/ViewModelStoreNavEntryDecoratorTest.kt, testViewModelNoSavedStateNavEntryDecorator
#[test]
fn a_view_model_needs_no_saveable_decorator() {
    let held = Held::default();
    let provider = noting(&held);
    let mut ui = compose_test(move || {
        let models = remember_view_model_store_nav_entry_decorator!();
        let entries = remember_decorated_nav_entries_from!(vec![provider("key1")], vec![models]);
        entries[0].Content();
    });
    ui.wait_for_idle();
    assert_eq!(held.borrow().last().unwrap().0, "key1");
}

thread_local! {
    static CREATED: Cell<u32> = const { Cell::new(0) };
}

struct CountingViewModel {
    base: ViewModelBase,
}

impl Default for CountingViewModel {
    fn default() -> Self {
        CREATED.set(CREATED.get() + 1);
        CountingViewModel { base: ViewModelBase::default() }
    }
}

impl ViewModel for CountingViewModel {
    fn base(&self) -> &ViewModelBase {
        &self.base
    }
}

// Compose: lifecycle/lifecycle-viewmodel-navigation3/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/navigation3/ViewModelStoreNavEntryDecoratorTest.kt, testViewModelSaved
#[test]
fn a_view_model_stays_while_its_entry_is_on_the_stack() {
    CREATED.set(0);
    let owner = mutable_state_list_from(["Home"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            entry_decorators = vec![
                remember_saveable_state_holder_nav_entry_decorator!(),
                remember_view_model_store_nav_entry_decorator!(),
            ],
            entry_provider = |key: &'static str| {
                NavEntry::new(key, |_| {
                    let _ = view_model::<CountingViewModel>();
                })
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(CREATED.get(), 1, "only one");
    stack.push("AnotherScreen");
    ui.wait_for_idle();
    assert_eq!(CREATED.get(), 2, "another");
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(CREATED.get(), 2, "the old one again");
    stack.push("AnotherScreen");
    ui.wait_for_idle();
    assert_eq!(CREATED.get(), 3, "another: that one was cleared");
}

// Compose: lifecycle/lifecycle-viewmodel-navigation3/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/navigation3/ViewModelStoreNavEntryDecoratorTest.kt, testViewModelRemovedOnPop
#[test]
fn a_view_model_is_cleared_when_its_entry_is_popped() {
    let held = Held::default();
    let provider = noting(&held);
    let owner = mutable_state_list_from([provider("1"), provider("2"), provider("3")]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let saveable = remember_saveable_state_holder_nav_entry_decorator!();
        let models = remember_view_model_store_nav_entry_decorator!();
        let entries = remember_decorated_nav_entries_from!(stack, vec![saveable, models]);
        for entry in &entries {
            key(entry.content_key().clone(), || entry.Content());
        }
    });
    ui.wait_for_idle();
    let cleared = |key: &str| view_model_of(&held, key).cleared.get();
    assert_eq!((cleared("1"), cleared("2"), cleared("3")), (false, false, false));
    stack.remove(2);
    ui.wait_for_idle();
    assert_eq!((cleared("1"), cleared("2"), cleared("3")), (false, false, true));
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!((cleared("1"), cleared("2"), cleared("3")), (false, true, true));
}

struct ExpectedOwner {
    store: ViewModelStore,
    extras: CreationExtras,
    factory: Rc<dyn ViewModelProviderFactory>,
}

impl ViewModelStoreOwner for ExpectedOwner {
    fn view_model_store(&self) -> ViewModelStore {
        self.store.clone()
    }

    fn default_view_model_provider_factory(&self) -> Rc<dyn ViewModelProviderFactory> {
        self.factory.clone()
    }

    fn default_view_model_creation_extras(&self) -> CreationExtras {
        self.extras.clone()
    }
}

// Compose: lifecycle/lifecycle-viewmodel-navigation3/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/navigation3/ViewModelStoreNavEntryDecoratorTest.kt, testHasDefaultViewModelProviderFactoryPropagation
#[test]
fn an_entry_s_view_model_uses_the_owner_s_defaults() {
    let key = CreationExtrasKey::<&'static str>::new();
    let mut extras = CreationExtras::new();
    extras.set(key, "customValue");
    let seen: Rc<RefCell<Option<&'static str>>> = Rc::default();
    let noted = seen.clone();
    let factory: Rc<dyn ViewModelProviderFactory> = view_model_factory(move |factory| {
        let noted = noted.clone();
        factory.initializer(move |extras: &CreationExtras| {
            *noted.borrow_mut() = extras.get(key);
            MyViewModel::default()
        });
    });
    let owner: ViewModelStoreOwnerRef =
        Rc::new(ExpectedOwner { store: ViewModelStore::new(), extras, factory });
    let entry = NavEntry::new("key", |_| {
        let _ = view_model::<MyViewModel>();
    });
    let mut ui = compose_test(move || {
        let saveable = remember_saveable_state_holder_nav_entry_decorator!();
        let models = remember_view_model_store_nav_entry_decorator!(owner.clone());
        let entries =
            remember_decorated_nav_entries_from!(vec![entry.clone()], vec![saveable, models]);
        entries[0].Content();
    });
    ui.wait_for_idle();
    assert_eq!(*seen.borrow(), Some("customValue"));
}

#[composable]
fn SharedScreen(name: &'static str, held: Held) {
    let model = view_model::<MyViewModel>();
    if model.my_arg.borrow().is_empty() {
        *model.my_arg.borrow_mut() = name;
    }
    held.borrow_mut().push((name, model));
}

#[composable]
fn Swapping(active: MutableState<bool>, held: Held) {
    let provider_a = remember_view_model_store_provider!(key = Some(1));
    let provider_b = remember_view_model_store_provider!(key = Some(2));
    let stack_a = remember(|| mutable_state_list_from(["SharedScreen"]));
    let stack_b = remember(|| mutable_state_list_from(["SharedScreen"]));
    if active.get() {
        let held = held.clone();
        NavDisplay!(
            stack_a,
            entry_decorators = vec![
                remember_saveable_state_holder_nav_entry_decorator!(),
                remember_view_model_store_nav_entry_decorator_of!(provider_a.clone()),
            ],
            entry_provider = move |key: &'static str| {
                let held = held.clone();
                NavEntry::new(key, move |_| SharedScreen!("state_a", held.clone()))
            }
        );
    } else {
        let held = held.clone();
        NavDisplay!(
            stack_b,
            entry_decorators = vec![
                remember_saveable_state_holder_nav_entry_decorator!(),
                remember_view_model_store_nav_entry_decorator_of!(provider_b.clone()),
            ],
            entry_provider = move |key: &'static str| {
                let held = held.clone();
                NavEntry::new(key, move |_| SharedScreen!("state_b", held.clone()))
            }
        );
    }
}

// Compose: lifecycle/lifecycle-viewmodel-navigation3/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/navigation3/ViewModelStoreNavEntryDecoratorTest.kt, testHoistedViewModelStoreProviders_isolateStateAndSurviveNavDisplaySwap
#[test]
fn hoisted_providers_keep_each_stack_s_view_models_while_swapped() {
    let held = Held::default();
    let active = mutable_state_of(true);
    let (read, given) = (active.handle(), held.clone());
    let mut ui = compose_test(move || Swapping!(read, given.clone()));
    ui.wait_for_idle();
    let a = view_model_of(&held, "state_a");
    assert_eq!(*a.my_arg.borrow(), "state_a");
    active.set(false);
    ui.wait_for_idle();
    let b = view_model_of(&held, "state_b");
    assert_eq!(*b.my_arg.borrow(), "state_b");
    assert!(!Rc::ptr_eq(&a, &b), "the same key, in other providers");
    active.set(true);
    ui.wait_for_idle();
    let again = view_model_of(&held, "state_a");
    assert!(Rc::ptr_eq(&a, &again), "kept through the swap");
    assert_eq!(*again.my_arg.borrow(), "state_a");
    assert!(!a.cleared.get());
    assert!(!b.cleared.get());
}
