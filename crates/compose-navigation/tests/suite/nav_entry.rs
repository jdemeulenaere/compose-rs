//! `NavEntry`, entry providers and metadata, against Navigation 3's `EntryTest`, `NavEntryTest`,
//! `EntryProviderTest`, `NavMetadataTest` and `SaveableStateHolderNavEntryDecoratorTest`.

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_foundation::BasicText;
use compose_navigation::*;
use compose_runtime::snapshots::mutable_state_list_from;
use compose_runtime::*;
use compose_test::*;

fn panic_text(body: impl FnOnce()) -> String {
    let payload = catch_unwind(AssertUnwindSafe(body)).expect_err("it should have panicked");
    match payload.downcast::<String>() {
        Ok(text) => *text,
        Err(payload) => payload.downcast::<&str>().map(|text| text.to_string()).unwrap_or_default(),
    }
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryTest.kt, getKey
#[test]
fn an_entry_has_the_content_key_it_was_given() {
    let entry = NavEntry::new("myKey", |_| {}).with_content_key("myKey");
    assert_eq!(*entry.content_key(), ContentKey::new("myKey"));
    assert_eq!(entry.content_key().downcast_ref::<String>().unwrap(), "myKey");
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryTest.kt, getFeatureMap
#[test]
fn an_entry_has_the_metadata_it_was_given() {
    #[derive(PartialEq, Debug)]
    struct Marker;
    let metadata = NavMetadata::new().with("feature1", 1).with("feature2", Marker);
    let entry = NavEntry::new("myKey", |_| {}).with_metadata(metadata);
    assert_eq!(entry.metadata().get_named::<i32>("feature1"), Some(&1));
    assert_eq!(entry.metadata().get_named::<Marker>("feature2"), Some(&Marker));
    assert_eq!(entry.metadata().get_named::<String>("feature1"), None);
}

#[derive(Clone, PartialEq, Debug)]
struct Home;

/// Kotlin's `TestSubinterface1.Home` and `TestSubinterface2.Home`: printing the same.
mod hierarchy {
    pub mod first {
        #[derive(Clone, PartialEq, Debug)]
        pub struct Home;
    }
    pub mod second {
        #[derive(Clone, PartialEq, Debug)]
        pub struct Home;
    }
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/NavEntryTest.kt, getContentKey_dataObject
#[test]
fn keys_printing_the_same_of_other_types_have_other_content_keys() {
    let first = NavEntry::new(hierarchy::first::Home, |_| {});
    let second = NavEntry::new(hierarchy::second::Home, |_| {});
    assert_ne!(first.content_key(), second.content_key());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/NavEntryTest.kt, getContentKey_dataClass
#[test]
fn keys_of_other_values_have_other_content_keys() {
    #[derive(Clone, PartialEq, Debug)]
    struct Data(i32);
    let (one, two) = (NavEntry::new(Data(1), |_| {}), NavEntry::new(Data(2), |_| {}));
    assert_ne!(one.content_key(), two.content_key());
    assert_eq!(one.content_key(), NavEntry::new(Data(1), |_| {}).content_key());
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryProviderTest.kt, entryProvider_withUniqueInitializers_returnsEntries
#[test]
fn a_provider_gives_each_key_its_entry() {
    let provider = entry_provider(|scope| {
        scope.entry("first", |_| {}).content_key("first");
        scope.entry("second", |_| {}).content_key("second");
    });
    assert_eq!(*provider("first").content_key(), ContentKey::new("first"));
    assert_eq!(*provider("second").content_key(), ContentKey::new("second"));
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryProviderTest.kt, entryProvider_properlyCreatesMetadata
#[test]
fn a_provider_makes_an_entry_s_metadata() {
    let provider = entry_provider(|scope| {
        scope
            .entry("first", |_| {})
            .content_key("first")
            .metadata_of(|_| NavMetadata::new().with("feature1", 1));
    });
    assert_eq!(provider("first").metadata().get_named::<i32>("feature1"), Some(&1));
}

#[derive(Clone, PartialEq, Debug)]
struct Named(&'static str);

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryProviderTest.kt, entryProvider_getKeyArgsFromMetadataWithObject
#[test]
fn an_entry_s_metadata_can_read_its_key() {
    let provider = entry_provider(|scope| {
        scope
            .entry(Named("testArg"), |_| {})
            .content_key("first")
            .metadata_of(|key| NavMetadata::new().with("arg1", key.0));
    });
    assert_eq!(provider(Named("testArg")).metadata().get_named::<&str>("arg1"), Some(&"testArg"));
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryProviderTest.kt, entryProvider_getKeyArgsFromMetadataWithClass
#[test]
fn a_class_s_metadata_can_read_its_key() {
    let provider = entry_provider(|scope| {
        scope
            .entry_of::<Named, _>(|_| {})
            .metadata_of(|key| NavMetadata::new().with("arg1", key.0));
    });
    assert_eq!(provider(Named("testArg")).metadata().get_named::<&str>("arg1"), Some(&"testArg"));
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryProviderTest.kt, entryProvider_withDuplicatedInitializers_throwsException
#[test]
fn a_key_added_twice_panics() {
    let text = panic_text(|| {
        let _ = entry_provider(|scope| {
            scope.entry(1, |_| {});
            scope.entry(1, |_| {});
        });
    });
    assert_eq!(text, "An `entry` with the key `key` has already been added: 1.");
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryProviderTest.kt, entryProvider_noInitializers_getsInvalidEntry
#[test]
fn a_key_with_no_entry_panics() {
    let provider = entry_provider::<i32>(|_| {});
    let text = panic_text(|| {
        provider(7);
    });
    assert_eq!(text, "Unknown screen 7");
}

/// Its `fallback` makes the entry instead.
#[test]
fn a_fallback_makes_the_entry_of_a_key_with_none() {
    let provider = entry_provider_with_fallback(
        |key: i32| NavEntry::new(key, |_| {}).with_content_key("fallback"),
        |scope| {
            scope.entry(1, |_| {}).content_key("one");
        },
    );
    assert_eq!(*provider(1).content_key(), ContentKey::new("one"));
    assert_eq!(*provider(2).content_key(), ContentKey::new("fallback"));
}

/// A class added twice panics, as a key does.
#[test]
fn a_class_added_twice_panics() {
    let text = panic_text(|| {
        let _ = entry_provider::<i32>(|scope| {
            scope.entry_of::<i32, _>(|_| {});
            scope.entry_of::<i32, _>(|_| {});
        });
    });
    assert!(text.starts_with("An `entry` with the same `clazz` has already been added"), "{text}");
}

struct LambdaKey;

impl NavMetadataKey for LambdaKey {
    type Value = Rc<dyn Fn() -> String>;
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryProviderTest.kt, entryProvider_nestedMetadataWithLambda
#[test]
fn a_key_s_metadata_is_made_once() {
    let provider = entry_provider(|scope| {
        scope.entry_of::<Home, _>(|_| {}).metadata_of(|_| {
            metadata(|scope| scope.put(&LambdaKey, Rc::new(|| "First".to_string())))
        });
    });
    let (first, second) = (provider(Home), provider(Home));
    assert_eq!(first, second);
    assert_eq!(first.metadata(), second.metadata());
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryProviderTest.kt, entryProvider_nestedMetadataWithLambda_dynamicLambdaReturnValue
#[test]
fn a_closure_in_metadata_reads_what_it_reads_now() {
    let value = Rc::new(std::cell::RefCell::new("start".to_string()));
    let read = value.clone();
    let provider = entry_provider(|scope| {
        scope.entry_of::<Home, _>(|_| {}).metadata_of(move |_| {
            let read = read.clone();
            metadata(|scope| scope.put(&LambdaKey, Rc::new(move || read.borrow().clone())))
        });
    });
    let first = provider(Home);
    assert_eq!((first.metadata().get(&LambdaKey).unwrap())(), "start");
    *value.borrow_mut() = "end".to_string();
    let second = provider(Home);
    assert_eq!((second.metadata().get(&LambdaKey).unwrap())(), "end");
    assert_eq!(first, second);
    assert_eq!(first.metadata(), second.metadata());
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryProviderTest.kt, entryProvider_prioritizeKeyInstance
#[test]
fn a_key_s_own_entry_goes_before_its_class_s() {
    let provider = entry_provider(|scope| {
        scope.entry(Named("testArg"), |_| {}).content_key("first");
        scope.entry_of::<Named, _>(|_| {}).content_key_of(|_| "second");
    });
    assert_eq!(*provider(Named("testArg")).content_key(), ContentKey::new("first"));
    assert_eq!(*provider(Named("other")).content_key(), ContentKey::new("second"));
}

/// Keys of an enum with a class for a variant: what isn't of it has none.
#[test]
fn a_class_takes_only_the_keys_of_it() {
    #[derive(Clone, PartialEq, Debug)]
    enum Screen {
        Details(Named),
        Other,
    }
    impl KeyClass<Screen> for Named {
        fn from_key(key: &Screen) -> Option<Named> {
            match key {
                Screen::Details(named) => Some(named.clone()),
                Screen::Other => None,
            }
        }
    }
    let shown = Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = shown.clone();
    let provider = entry_provider_with_fallback(
        |key: Screen| NavEntry::new(key, |_| {}).with_content_key("fallback"),
        |scope| {
            scope
                .entry_of::<Named, _>(move |key: Named| seen.borrow_mut().push(key.0))
                .content_key_of(|key: &Named| key.0);
        },
    );
    let entry = provider(Screen::Details(Named("a")));
    assert_eq!(*entry.content_key(), ContentKey::new("a"));
    assert_eq!(*provider(Screen::Other).content_key(), ContentKey::new("fallback"));
    let mut ui = compose_test(move || entry.Content());
    ui.wait_for_idle();
    assert_eq!(*shown.borrow(), ["a"]);
}

#[derive(PartialEq, Debug)]
struct Payload;

struct ObjectKey;

impl NavMetadataKey for ObjectKey {
    type Value = Payload;
}

struct Numbered(u32);

impl NavMetadataKey for Numbered {
    type Value = char;
    fn name(&self) -> String {
        format!("Numbered{}", self.0)
    }
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/NavMetadataTest.kt, testKey_objectKey
#[test]
fn a_value_is_read_by_its_key() {
    let metadata = metadata(|scope| scope.put(&ObjectKey, Payload));
    assert_eq!(metadata.get(&ObjectKey), Some(&Payload));
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/NavMetadataTest.kt, testKey_classKey
#[test]
fn keys_of_one_type_hold_their_own_values() {
    let metadata = metadata(|scope| {
        scope.put(&Numbered(1), 'a');
        scope.put(&Numbered(2), 'b');
    });
    assert_eq!(metadata.get(&Numbered(1)), Some(&'a'));
    assert_eq!(metadata.get(&Numbered(2)), Some(&'b'));
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/NavMetadataTest.kt, testKey_invalidKey
#[test]
fn a_key_never_put_reads_nothing() {
    struct WrongKey;
    impl NavMetadataKey for WrongKey {
        type Value = Payload;
    }
    let metadata = metadata(|scope| scope.put(&ObjectKey, Payload));
    assert_eq!(metadata.get(&WrongKey), None);
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/NavMetadataTest.kt, testKey_differentKeyTypes
#[test]
fn keys_of_other_types_live_side_by_side() {
    let metadata = metadata(|scope| {
        scope.put(&ObjectKey, Payload);
        scope.put(&Numbered(1), 'a');
    });
    assert_eq!(metadata.get(&ObjectKey), Some(&Payload));
    assert_eq!(metadata.get(&Numbered(1)), Some(&'a'));
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/NavMetadataTest.kt, testValue_lambdaValue
#[test]
fn a_value_can_be_a_closure() {
    struct One;
    struct Two;
    impl NavMetadataKey for One {
        type Value = Rc<dyn Fn() -> i32>;
    }
    impl NavMetadataKey for Two {
        type Value = Rc<dyn Fn() -> i32>;
    }
    let metadata = metadata(|scope| {
        scope.put(&One, Rc::new(|| 1));
        scope.put(&Two, Rc::new(|| 2));
    });
    assert_eq!((metadata.get(&One).unwrap())(), 1);
    assert_eq!((metadata.get(&Two).unwrap())(), 2);
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/NavMetadataTest.kt, testOverrideExistingValue
#[test]
fn the_last_value_of_a_key_stays() {
    let one = metadata(|scope| {
        scope.put(&Numbered(1), 'a');
        scope.put(&Numbered(1), 'b');
    });
    assert_eq!(one.get(&Numbered(1)), Some(&'b'));
    let merged = metadata(|scope| scope.put(&Numbered(1), 'a'))
        + metadata(|scope| scope.put(&Numbered(1), 'b'));
    assert_eq!(merged.get(&Numbered(1)), Some(&'b'));
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/NavMetadataTest.kt, testPlus
#[test]
fn metadata_merges_with_other_metadata() {
    let merged =
        metadata(|scope| scope.put(&Numbered(1), 'a')) + NavMetadata::new().with("key", "value");
    assert_eq!(merged.get(&Numbered(1)), Some(&'a'));
    assert_eq!(merged.get_named::<&str>("key"), Some(&"value"));
    let merged = metadata(|scope| scope.put(&Numbered(1), 'a'))
        + metadata(|scope| scope.put(&Numbered(2), 'b'));
    assert_eq!(merged.get(&Numbered(1)), Some(&'a'));
    assert_eq!(merged.get(&Numbered(2)), Some(&'b'));
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/NavMetadataTest.kt, testContains
#[test]
fn metadata_says_what_it_holds() {
    let metadata = metadata(|scope| scope.put(&Numbered(1), 'a'));
    assert!(metadata.contains(&Numbered(1)));
    assert!(!metadata.contains(&Numbered(2)));
}

#[derive(Clone, PartialEq, Debug)]
enum Data {
    One,
    Two,
}

thread_local! {
    /// The counter of what's on screen, as the test's `numberOnScreen1`.
    static COUNTER: Cell<Option<MutableState<i32>>> = const { Cell::new(None) };
}

#[composable]
fn CountingScreen() {
    let count = remember_saveable(|| mutable_state_of(0));
    COUNTER.set(Some(count));
    BasicText!("numberOnScreen1: {}", count.get());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/SaveableStateHolderNavEntryDecoratorTest.kt, testDataClassSamePropertyNotDuplicates
#[test]
fn entries_keep_their_own_saved_state_and_lose_it_when_popped() {
    let owner = mutable_state_list_from([Data::One]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let decorator = remember_saveable_state_holder_nav_entry_decorator!();
        let entries = remember_decorated_nav_entries!(
            stack,
            vec![decorator],
            entry_provider = |key: Data| NavEntry::new(key, |_| CountingScreen())
        );
        entries.last().unwrap().Content();
    });
    let counter = || COUNTER.get().unwrap();
    ui.wait_for_idle();
    assert_eq!(counter().get(), 0);
    counter().set(2);
    ui.wait_for_idle();
    ui.on_node_with_text("numberOnScreen1: 2").assert_is_displayed();

    stack.push(Data::Two);
    ui.wait_for_idle();
    assert_eq!(counter().get(), 0, "another entry's own");
    counter().set(4);
    ui.wait_for_idle();
    ui.on_node_with_text("numberOnScreen1: 4").assert_is_displayed();

    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(counter().get(), 2, "the entry below's, restored");
    ui.on_node_with_text("numberOnScreen1: 2").assert_is_displayed();

    stack.push(Data::Two);
    ui.wait_for_idle();
    assert_eq!(counter().get(), 0, "what was popped is forgotten");
    ui.on_node_with_text("numberOnScreen1: 0").assert_is_displayed();
}

thread_local! {
    static REMEMBERED: Cell<u32> = const { Cell::new(0) };
    static NEXT: Cell<u32> = const { Cell::new(0) };
}

/// What a place remembers: a new number each time it's new.
fn remembering() {
    REMEMBERED.set(remember(|| {
        NEXT.set(NEXT.get() + 1);
        NEXT.get()
    }));
}

/// A closure's content is in a group by the closure, as a composable lambda by its place in the
/// source: another closure's replaces it at the same position, and another key of the same doesn't.
#[test]
fn another_closure_s_content_starts_anew_and_another_key_doesn_t() {
    NEXT.set(0);
    let (one, two, three) = (
        NavEntry::new(1, |_| remembering()),
        NavEntry::new(2, |_: i32| remembering()),
        NavEntry::new(3, |_| {
            remembering();
        }),
    );
    let same_closure = |_: i32| remembering();
    let (four, five) = (NavEntry::new(4, same_closure), NavEntry::new(5, same_closure));
    let shown = mutable_state_of(0);
    let read = shown.handle();
    let entries = [one, two, three, four, five];
    let mut ui = compose_test(move || entries[read.get()].Content());
    ui.wait_for_idle();
    assert_eq!(REMEMBERED.get(), 1);
    shown.set(1);
    ui.wait_for_idle();
    assert_eq!(REMEMBERED.get(), 2, "another closure");
    shown.set(2);
    ui.wait_for_idle();
    assert_eq!(REMEMBERED.get(), 3, "another closure");
    shown.set(3);
    ui.wait_for_idle();
    assert_eq!(REMEMBERED.get(), 4, "another closure");
    shown.set(4);
    ui.wait_for_idle();
    assert_eq!(REMEMBERED.get(), 4, "the same closure, with another key");
}

/// An entry's content is a scope of its own: what it reads recomposes it alone.
#[test]
fn an_entry_s_content_recomposes_on_its_own() {
    let (state, outer_runs, inner_runs) =
        (mutable_state_of(0), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (read, counted) = (state.handle(), inner_runs.clone());
    let entry = NavEntry::new(1, move |_| {
        counted.set(counted.get() + 1);
        let _ = read.get();
    });
    let outer = outer_runs.clone();
    let mut ui = compose_test(move || {
        outer.set(outer.get() + 1);
        entry.Content();
    });
    ui.wait_for_idle();
    assert_eq!((outer_runs.get(), inner_runs.get()), (1, 1));
    state.set(1);
    ui.wait_for_idle();
    assert_eq!((outer_runs.get(), inner_runs.get()), (1, 2));
}
