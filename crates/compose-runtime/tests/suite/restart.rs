//! Compose's `RestartTests` and `ModelViewTests`, adapted: composables and content lambdas
//! reading model state restart; a restarted scope skips the children whose arguments didn't
//! change.

#![allow(non_snake_case)]

use std::rc::Rc;

use compose_runtime::*;

use crate::mock::*;

const PRESIDENT_NAME_1: &str = "George Washington";
const PRESIDENT_AGE_1: i32 = 57;
const PRESIDENT_NAME_16: &str = "Abraham Lincoln";
const PRESIDENT_AGE_16: i32 = 52;

/// A person whose name and age are state, as the tests' `Person`.
struct Person {
    name: OwnedState<String>,
    age: OwnedState<i32>,
}

type PersonRef = Rc<Person>;

fn person(name: &str, age: i32) -> PersonRef {
    Rc::new(Person { name: mutable_state_of(name.to_string()), age: mutable_state_of(age) })
}

impl Person {
    fn name(&self) -> String {
        self.name.get()
    }

    fn age(&self) -> i32 {
        self.age.get()
    }

    fn become_president_16(&self) {
        self.name.set(PRESIDENT_NAME_16.into());
        self.age.set(PRESIDENT_AGE_16);
    }
}

#[composable]
fn RestartGroup(content: impl Fn() + 'static) {
    content();
}

#[composable]
fn Repeat(count: i32, block: impl Fn(i32) + 'static) {
    for index in 0..count {
        block(index);
    }
}

/// The person's name and age as texts.
fn expect_person(v: &mut Expected, person: &Person) {
    v.text(&person.name());
    v.text(&person.age().to_string());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RestartTests.kt, restart_PersonModel_lambda
#[test]
fn a_content_lambda_reading_a_model_restarts() {
    #[composable]
    fn Content(president: PersonRef) {
        RestartGroup!({
            Text(&president.name());
            Text(&president.age().to_string());
        });
    }
    let mut test = composition_test();
    let president = person(PRESIDENT_NAME_1, PRESIDENT_AGE_1);
    let p = president.clone();
    test.compose(move || Content(p.clone()));
    let shown = president.clone();
    test.validate(move |v| expect_person(v, &shown));
    president.become_president_16();
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RestartTests.kt, restart_PersonModel_lambda_parameters
#[test]
fn a_content_lambda_with_parameters_reading_a_model_restarts() {
    #[composable]
    fn Content(president: PersonRef) {
        Repeat!(5, |_index: i32| {
            Text(&president.name());
            Text(&president.age().to_string());
        });
    }
    let mut test = composition_test();
    let president = person(PRESIDENT_NAME_1, PRESIDENT_AGE_1);
    let p = president.clone();
    test.compose(move || Content(p.clone()));
    let shown = president.clone();
    test.validate(move |v| {
        for _ in 0..5 {
            expect_person(v, &shown);
        }
    });
    president.become_president_16();
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RestartTests.kt, restart_PersonModel_function
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ModelViewTests.kt, testModelView_PersonModel
#[test]
fn a_composable_reading_a_model_restarts() {
    #[composable]
    fn PersonView(president: PersonRef) {
        Text(&president.name());
        Text(&president.age().to_string());
    }
    let mut test = composition_test();
    let president = person(PRESIDENT_NAME_1, PRESIDENT_AGE_1);
    let p = president.clone();
    test.compose(move || PersonView(p.clone()));
    let shown = president.clone();
    test.validate(move |v| expect_person(v, &shown));
    president.become_president_16();
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RestartTests.kt, restart_State_delete
#[test]
fn a_restarted_composable_removes_and_inserts_its_child() {
    #[composable]
    fn ShowSomething(state: MutableState<bool>) {
        Text(&format!("State = {}", state.get()));
    }
    #[composable]
    fn View(state: MutableState<bool>) {
        if state.get() {
            ShowSomething(state);
        }
    }
    let mut test = composition_test();
    let state = mutable_state_of(true);
    let s = state.handle();
    test.compose(move || View(s));
    test.validate(move |v| {
        if s.get() {
            v.text(&format!("State = {}", s.get()));
        }
    });
    state.set(false);
    test.expect_changes();
    test.revalidate();
    state.set(true);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RestartTests.kt, restart_PersonModel_function_parameters
#[test]
fn composables_with_parameters_reading_a_model_restart() {
    #[composable]
    fn PersonView(index: i32, president: PersonRef) {
        Text(&index.to_string());
        Text(&president.name());
        Text(&president.age().to_string());
    }
    #[composable]
    fn Content(president: PersonRef) {
        Repeat!(5, |index: i32| PersonView(index, president.clone()));
    }
    let mut test = composition_test();
    let president = person(PRESIDENT_NAME_1, PRESIDENT_AGE_1);
    let p = president.clone();
    test.compose(move || Content(p.clone()));
    let shown = president.clone();
    test.validate(move |v| {
        for index in 0..5 {
            v.text(&index.to_string());
            expect_person(v, &shown);
        }
    });
    president.become_president_16();
    test.expect_changes();
    test.revalidate();
}

#[composable]
fn RestartAndSkipTest(count: i32, data: State<i32>, calls: Rc<(Counter, Counter)>) {
    RestartAndSkipTest_UseCount(count, data, calls);
}

#[composable]
fn RestartAndSkipTest_UseCount(count: i32, data: State<i32>, calls: Rc<(Counter, Counter)>) {
    Text(&format!("Data: {}, Count: {count}", data.get()));
    calls.0.increment();
    RestartAndSkipTest_UseCount2(count, calls.clone());
}

#[composable]
fn RestartAndSkipTest_UseCount2(count: i32, calls: Rc<(Counter, Counter)>) {
    Text(&format!("Count: {count}"));
    calls.1.increment();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RestartTests.kt, restart_and_skip
#[test]
fn a_restarted_scope_skips_unchanged_children() {
    let mut test = composition_test();
    let count = mutable_state_of(0);
    let data = mutable_state_of(0);
    let calls = Rc::new((Counter::new(), Counter::new()));
    let (c, d, k) = (count.handle(), data.as_state(), calls.clone());
    test.compose(move || RestartAndSkipTest(c.get(), d, k.clone()));
    assert_eq!((calls.0.get(), calls.1.get()), (1, 1));
    test.validate(move |v| {
        v.text(&format!("Data: {}, Count: {}", d.get(), c.get()));
        v.text(&format!("Count: {}", c.get()));
    });
    count.update(|count| count + 1);
    test.expect_changes();
    test.revalidate();
    assert_eq!((calls.0.get(), calls.1.get()), (2, 2));
    data.update(|data| data + 1);
    test.expect_changes();
    test.revalidate();
    assert_eq!((calls.0.get(), calls.1.get()), (3, 2), "the second child skipped");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ModelViewTests.kt, testModelView_Simple
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ModelViewTests.kt, testModelView_Simple_Recompose
#[test]
fn a_static_text_changes_nothing_when_recomposed() {
    let mut test = composition_test();
    test.compose(|| Text("Hello world!"));
    test.validate(|v| v.text("Hello world!"));
    test.expect_no_changes();
    test.revalidate();
}

#[composable]
fn Display(person: PersonRef) {
    Text(&person.name());
    Text(&person.age().to_string());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ModelViewTests.kt, testModelView_RecomposeScopeCleanup
#[test]
fn a_removed_composables_scope_is_cleaned_up() {
    #[composable]
    fn Content(washington: PersonRef, lincoln: PersonRef, display_lincoln: MutableState<bool>) {
        Display(washington);
        if display_lincoln.get() {
            Display(lincoln);
        }
    }
    let mut test = composition_test();
    let washington = person(PRESIDENT_NAME_1, PRESIDENT_AGE_1);
    let lincoln = person(PRESIDENT_NAME_16, PRESIDENT_AGE_16);
    let display_lincoln = mutable_state_of(true);
    let (w, l, d) = (washington.clone(), lincoln.clone(), display_lincoln.handle());
    test.compose(move || Content(w.clone(), l.clone(), d));
    let (w, l) = (washington.clone(), lincoln.clone());
    test.validate(move |v| {
        expect_person(v, &w);
        if d.get() {
            expect_person(v, &l);
        }
    });
    display_lincoln.set(false);
    test.expect_changes();
    test.revalidate();
    // The removed scope reads nothing any more.
    lincoln.name.set("Somebody else".into());
    test.expect_no_changes();
}

#[composable]
fn ConditionalDisplay(person: PersonRef) {
    Text(&person.name());
    Text(&person.age().to_string());
    if person.name() == PRESIDENT_NAME_16 {
        Text(&person.name());
        Text(&person.age().to_string());
    }
}

/// `ConditionalDisplay`'s views for `name`.
fn expect_conditional(v: &mut Expected, name: &str, age: i32) {
    v.text(name);
    v.text(&age.to_string());
    if name == PRESIDENT_NAME_16 {
        v.text(name);
        v.text(&age.to_string());
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ModelViewTests.kt, testObserverEntering
#[test]
fn content_entering_with_a_change_reads_the_model() {
    let mut test = composition_test();
    let president = person(PRESIDENT_NAME_1, PRESIDENT_AGE_1);
    let p = president.clone();
    test.compose(move || ConditionalDisplay(p.clone()));
    let shown = president.clone();
    test.validate(move |v| expect_conditional(v, &shown.name(), shown.age()));
    president.name.set(PRESIDENT_NAME_16.into());
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ModelViewTests.kt, testModelUpdatesNextFrameVisibility
#[test]
fn a_model_change_shows_at_the_next_frame() {
    let mut test = composition_test();
    let president = person(PRESIDENT_NAME_1, PRESIDENT_AGE_1);
    let p = president.clone();
    test.compose(move || ConditionalDisplay(p.clone()));
    president.name.set(PRESIDENT_NAME_16.into());
    // Not there yet: the next frame recomposes.
    test.validate(|v| expect_conditional(v, PRESIDENT_NAME_1, PRESIDENT_AGE_1));
    test.expect_changes();
    let shown = president.clone();
    test.validate(move |v| expect_conditional(v, &shown.name(), shown.age()));
}
